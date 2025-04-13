use crate::audio_recoder::{self};
use crate::transcribe_audio;
use std::error::Error;

pub async fn start_recording() -> Result<(), Box<dyn Error>> {
    audio_recoder::record_with_duration(None).await
}

pub async fn stop_recording_and_transcribe() -> Result<String, Box<dyn Error>> {
    let filename = audio_recoder::stop_recording().await?;
    transcribe_audio::transcribe_audio(&filename).await
}
