pub mod capture;
pub mod client;
pub mod signature;

pub use capture::{
    CaptureTool, available_tool, capture_seconds, default_capture_path, read_wav_mono16k,
};
pub use client::{
    MAX_ATTEMPTS, MIN_INTERVAL_MS, RecognizedLinks, RecognitionOutcome, ShazamClient, ShazamMatch,
    TIMEZONES, USER_AGENTS, parse_response_body, youtube_video_id,
};
pub use signature::{SAMPLE_RATE_HZ, Signature, SignatureGenerator, crc32};

pub fn record_match(
    conn: &rusqlite::Connection,
    recognition: &madrigal_core::Recognition,
) -> madrigal_db::Result<()> {
    madrigal_db::record_recognition(conn, recognition)
}

pub fn default_capture_seconds() -> u32 {
    madrigal_settings::Settings::default()
        .recognition
        .capture_seconds
}
