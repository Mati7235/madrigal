use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use innertube_rs::constants::clients::VISIONOS_USER_AGENT;
use innertube_rs::endpoints::player::{resolve_stream_url, select_format};
use innertube_rs::models::format::{FormatFilter, FormatType, QualityPreference};
use innertube_rs::models::video::GetVideoInfoOptions;
use innertube_rs::Innertube;

use crate::resolver::expiry_from_url;

pub const CLIENT: &str = "VISIONOS";
pub const LABEL: &str = "innertube:visionos";

#[derive(Debug, Clone, PartialEq)]
pub struct InnertubeStream {
    pub url: String,
    pub user_agent: Option<String>,
    pub ext: Option<String>,
    pub acodec: Option<String>,
    pub expires_at: Option<f64>,
}

pub trait InnertubeResolver: Send + Sync {
    fn resolve(&self, video_id: &str) -> Result<InnertubeStream, String>;
}

type Client = (tokio::runtime::Runtime, Innertube);

pub struct VisionosResolver {
    timeout: Duration,
    client: Mutex<Option<Client>>,
}

impl VisionosResolver {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout: timeout.max(Duration::from_secs(1)),
            client: Mutex::new(None),
        }
    }

    fn client(&self) -> Result<MutexGuard<'_, Option<Client>>, String> {
        let mut guard = self
            .client
            .lock()
            .map_err(|_| "innertube client poisoned".to_string())?;
        if guard.is_none() {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|err| format!("innertube runtime: {err}"))?;
            let client = runtime.block_on(async {
                tokio::time::timeout(self.timeout, Innertube::new())
                    .await
                    .map_err(|_| "innertube init timed out".to_string())?
                    .map_err(|err| format!("innertube init: {err}"))
            })?;
            *guard = Some((runtime, client));
        }
        Ok(guard)
    }
}

impl Default for VisionosResolver {
    fn default() -> Self {
        Self::new(Duration::from_secs(15))
    }
}

impl InnertubeResolver for VisionosResolver {
    fn resolve(&self, video_id: &str) -> Result<InnertubeStream, String> {
        let guard = self.client()?;
        let (runtime, client) = guard.as_ref().ok_or("innertube client missing")?;
        runtime.block_on(async {
            let options = GetVideoInfoOptions {
                client: Some(CLIENT.to_string()),
                ..GetVideoInfoOptions::default()
            };
            let info = tokio::time::timeout(
                self.timeout,
                client.get_basic_info(video_id, Some(&options)),
            )
            .await
            .map_err(|_| "innertube request timed out".to_string())?
            .map_err(|err| format!("innertube: {err}"))?;
            let status = &info.player_response.playability_status;
            if status.status != "OK" {
                let reason = status.reason.as_deref().unwrap_or("unknown");
                return Err(format!("innertube: {reason}"));
            }
            let filter = FormatFilter {
                format_type: FormatType::AudioOnly,
                quality: QualityPreference::Highest,
                container: None,
            };
            let format = select_format(&info.player_response, &filter)
                .map_err(|err| format!("innertube: {err}"))?;
            let url = resolve_stream_url(format, &client.player.decipherer)
                .map_err(|err| format!("innertube: {err}"))?;
            Ok(InnertubeStream {
                expires_at: expiry_from_url(&url),
                url,
                user_agent: Some(VISIONOS_USER_AGENT.to_string()),
                ext: codec_label(format.mime_type.as_str(), false),
                acodec: codec_label(format.mime_type.as_str(), true),
            })
        })
    }
}

fn codec_label(mime_type: &str, codec: bool) -> Option<String> {
    let (container, params) = mime_type.split_once(';')?;
    let extension = match container.trim() {
        "audio/mp4" => "m4a",
        "audio/webm" => "webm",
        "audio/ogg" => "ogg",
        other => other.rsplit_once('/')?.1,
    };
    if !codec {
        return Some(extension.to_string());
    }
    let value = params.split_once("codecs=")?.1.trim().trim_matches('"');
    (!value.is_empty()).then(|| value.to_string())
}
