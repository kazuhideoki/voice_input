use crate::audio_recoder::{self};
use crate::transcribe_audio;
use crate::{sound_player, text_selection};
use std::error::Error;

pub async fn start_recording() -> Result<(Option<String>, ()), Box<dyn Error>> {
    sound_player::pause_apple_music(); // 音楽を停止

    let selected_text = text_selection::get_selected_text().ok();
    audio_recoder::record_with_duration(None).await?;
    Ok((selected_text, ()))
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

    sound_player::resume_apple_music(); // 音楽を再生再開
    result
}
