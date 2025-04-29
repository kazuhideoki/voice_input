use crate::infrastructure::external::clipboard;
use std::error::Error;
use tokio::sync::mpsc;

pub async fn start_recording(
    _notify_timeout_tx: mpsc::Sender<()>,
) -> Result<Option<String>, Box<dyn Error>> {
    let selected_text = clipboard::get_selected_text().ok();

    Ok(selected_text)
}
