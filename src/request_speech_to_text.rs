use crate::audio_recoder::{self};
use std::error::Error;

pub async fn start_recording() -> Result<(), Box<dyn Error>> {
    audio_recoder::record_with_duration(None).await
}

pub async fn stop_recording_and_transcribe() -> Result<String, Box<dyn Error>> {
    audio_recoder::stop_recording().await
}
