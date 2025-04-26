use tokio::sync::mpsc;

use crate::audio_recoder::{self};
use crate::text_selection;
use std::error::Error;

pub async fn start_recording(
    notify_timeout_tx: mpsc::Sender<()>,
) -> Result<Option<String>, Box<dyn Error>> {
    let selected_text = text_selection::get_selected_text().ok();
    audio_recoder::start_recording(Some(30), notify_timeout_tx).await?;
    Ok(selected_text)
}
