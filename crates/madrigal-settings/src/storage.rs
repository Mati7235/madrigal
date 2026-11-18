use serde::{Deserialize, Serialize};

use crate::content::SongSortType;
use crate::de::{or_default, vec_filtered};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Storage {
    #[serde(deserialize_with = "or_default")]
    pub downloads_dir: String,
    #[serde(deserialize_with = "or_default")]
    pub download_concurrency: i32,
    #[serde(deserialize_with = "or_default")]
    pub max_image_cache_mb: i32,
    #[serde(deserialize_with = "or_default")]
    pub max_song_cache_mb: i32,
    #[serde(deserialize_with = "or_default")]
    pub max_canvas_cache_mb: i32,
    #[serde(deserialize_with = "or_default")]
    pub max_lyrics_cache_mb: i32,
    #[serde(deserialize_with = "or_default")]
    pub smart_trimmer: bool,
    #[serde(deserialize_with = "or_default")]
    pub storage_folder_path: String,
    #[serde(deserialize_with = "or_default")]
    pub storage_folder_display_name: String,
    #[serde(default, deserialize_with = "vec_filtered")]
    pub local_songs_included_folders: Vec<String>,
    #[serde(default, deserialize_with = "vec_filtered")]
    pub local_songs_excluded_folders: Vec<String>,
    #[serde(deserialize_with = "or_default")]
    pub local_songs_sort_type: SongSortType,
    #[serde(deserialize_with = "or_default")]
    pub local_songs_sort_descending: bool,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            downloads_dir: String::new(),
            download_concurrency: 2,
            max_image_cache_mb: 512,
            max_song_cache_mb: 4096,
            max_canvas_cache_mb: 512,
            max_lyrics_cache_mb: 64,
            smart_trimmer: true,
            storage_folder_path: String::new(),
            storage_folder_display_name: String::new(),
            local_songs_included_folders: Vec::new(),
            local_songs_excluded_folders: Vec::new(),
            local_songs_sort_type: SongSortType::CreateDate,
            local_songs_sort_descending: false,
        }
    }
}

fn clean_folders(raw: &[String]) -> Vec<String> {
    let mut folders = Vec::new();
    for folder in raw {
        let folder = folder.trim().to_owned();
        if !folder.is_empty() && !folders.contains(&folder) {
            folders.push(folder);
        }
    }
    folders
}

impl Storage {
    pub fn sanitize(&mut self) {
        self.download_concurrency =
            crate::sanitize::clamp_i32(self.download_concurrency, 1, 6);
        self.max_image_cache_mb =
            crate::sanitize::clamp_i32(self.max_image_cache_mb, 0, 1024 * 1024);
        self.max_song_cache_mb =
            crate::sanitize::clamp_i32(self.max_song_cache_mb, 0, 1024 * 1024);
        self.max_canvas_cache_mb =
            crate::sanitize::clamp_i32(self.max_canvas_cache_mb, 0, 1024 * 1024);
        self.max_lyrics_cache_mb =
            crate::sanitize::clamp_i32(self.max_lyrics_cache_mb, 0, 1024 * 1024);
        self.downloads_dir = self.downloads_dir.trim().to_owned();
        self.storage_folder_path = self.storage_folder_path.trim().to_owned();
        self.storage_folder_display_name =
            self.storage_folder_display_name.trim().to_owned();
        self.local_songs_included_folders = clean_folders(&self.local_songs_included_folders);
        self.local_songs_excluded_folders = clean_folders(&self.local_songs_excluded_folders);
    }
}
