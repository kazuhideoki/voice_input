use crate::audio_recoder::{self};
use crate::transcribe_audio;
use crate::{sound_player, text_selection};
use std::error::Error;

pub async fn start_recording() -> Result<(Option<String>, bool), Box<dyn Error>> {
    let was_playing = sound_player::pause_apple_music(); // 音楽を停止、再生中だったかどうかを返す

    let selected_text = text_selection::get_selected_text().ok();
    audio_recoder::record_with_duration(None).await?;
    Ok((selected_text, was_playing))
}

pub async fn stop_recording_and_transcribe(
    start_selected_text: Option<String>,
    was_music_playing: bool,
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

    // 録音開始時に音楽が再生されていた場合のみ再開
    if was_music_playing {
        sound_player::resume_apple_music();
    }
    
    result
}
