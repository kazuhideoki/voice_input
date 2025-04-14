use crate::audio_recoder::{self};
use crate::text_selection;
use crate::transcribe_audio;
use std::error::Error;

pub async fn start_recording() -> Result<Option<String>, Box<dyn Error>> {
    let selected_text = text_selection::get_selected_text().ok();
    audio_recoder::record_with_duration(None).await?;
    Ok(selected_text)
}

pub async fn stop_recording_and_transcribe(
    start_selected_text: Option<String>,
) -> Result<String, Box<dyn Error>> {
    let filename = audio_recoder::stop_recording().await?;

    // Use the selected text from recording start as prompt if available
    let result = match start_selected_text {
        Some(text) if !text.trim().is_empty() => {
            println!(
                "Using text selected at recording start as context: {:?}",
                text
            );
            transcribe_audio::transcribe_audio(&filename, Some(&text)).await
        }
        _ => transcribe_audio::transcribe_audio(&filename, None).await,
    };

    result
}
