//! テキスト直接入力モジュール
//!
//! 常駐ワーカーを使用してテキストを入力する機能を提供

use crate::infrastructure::external::text_input_worker::{
    TextInputEngine, TextInputWorkerError, TextInputWorkerHandle, start_text_input_worker,
};
use crate::utils::profiling;
use std::error::Error;
use std::sync::OnceLock;

static TEXT_INPUT_WORKER: OnceLock<TextInputWorkerHandle> = OnceLock::new();

/// テキスト入力ワーカーを初期化
pub fn init_worker() -> Result<(), TextInputWorkerError> {
    if TEXT_INPUT_WORKER.get().is_some() {
        return Ok(());
    }

    let handle = start_text_input_worker()?;
    let _ = TEXT_INPUT_WORKER.set(handle);
    Ok(())
}

/// メイン入力関数
///
/// 常駐ワーカーを使用してテキストを入力します。
///
/// # Example
/// ```no_run
/// # use voice_input::infrastructure::external::text_input::type_text;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// type_text("Hello, World!").await?;
/// # Ok(())
/// # }
/// ```
pub async fn type_text(text: &str) -> Result<(), Box<dyn Error>> {
    let handle = TEXT_INPUT_WORKER.get().ok_or_else(|| {
        TextInputWorkerError::ChannelClosed("text input worker not initialized".to_string())
    })?;

    let timer = profiling::Timer::start("text_input.worker");
    let result = handle.type_text(text).await;

    if profiling::enabled() {
        timer.log_with(&format!("ok={} text_len={}", result.is_ok(), text.len()));
    } else {
        timer.log();
    }

    result.map_err(|e| Box::new(e) as Box<dyn Error>)
}
