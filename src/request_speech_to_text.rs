use crate::audio_recoder::{self};
use crate::text_selection;
use crate::transcribe_audio;
use std::error::Error;

pub async fn start_recording() -> Result<(), Box<dyn Error>> {
    audio_recoder::record_with_duration(None).await
}

pub async fn stop_recording_and_transcribe() -> Result<String, Box<dyn Error>> {
    let filename = audio_recoder::stop_recording().await?;

    // Get selected text if available
    let selected_text = text_selection::get_selected_text().ok();

    // Use the selected text as prompt if available
    match selected_text {
        Some(text) if !text.trim().is_empty() => {
            println!("Using selected text as context: {:?}", text);
            transcribe_audio::transcribe_audio(&filename, Some(&text)).await
        }
        _ => transcribe_audio::transcribe_audio(&filename, None).await,
    }
}
