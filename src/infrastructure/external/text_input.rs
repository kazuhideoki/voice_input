//! テキスト直接入力モジュール
//!
//! 常駐ワーカーを使用してテキストを入力する機能を提供

use crate::infrastructure::external::text_input_worker::{
    TextInputEngine, TextInputWorkerError, TextInputWorkerHandle, start_text_input_worker,
};
use crate::utils::profiling;
use std::sync::{Mutex, OnceLock};

static TEXT_INPUT_WORKER: OnceLock<Mutex<Option<TextInputWorkerHandle>>> = OnceLock::new();

fn worker_slot() -> &'static Mutex<Option<TextInputWorkerHandle>> {
    TEXT_INPUT_WORKER.get_or_init(|| Mutex::new(None))
}

fn current_worker_handle() -> Result<TextInputWorkerHandle, TextInputWorkerError> {
    worker_slot()
        .lock()
        .map_err(|e| TextInputWorkerError::ChannelClosed(format!("worker lock poisoned: {}", e)))?
        .clone()
        .ok_or_else(|| {
            TextInputWorkerError::ChannelClosed("text input worker not initialized".to_string())
        })
}

fn replace_worker_handle() -> Result<TextInputWorkerHandle, TextInputWorkerError> {
    let handle = start_text_input_worker()?;
    let mut worker = worker_slot()
        .lock()
        .map_err(|e| TextInputWorkerError::ChannelClosed(format!("worker lock poisoned: {}", e)))?;
    *worker = Some(handle.clone());
    Ok(handle)
}

async fn run_with_recovery<F, Fut>(
    metric_name: &'static str,
    metric_suffix: String,
    f: F,
) -> Result<(), TextInputWorkerError>
where
    F: Fn(TextInputWorkerHandle) -> Fut,
    Fut: std::future::Future<Output = Result<(), TextInputWorkerError>>,
{
    let timer = profiling::Timer::start(metric_name);
    let mut result = f(current_worker_handle()?).await;
    if matches!(result, Err(TextInputWorkerError::ChannelClosed(_))) {
        let recovered = recover_after_wake();
        if recovered.is_ok() {
            result = f(current_worker_handle()?).await;
        } else if let Err(err) = recovered {
            result = Err(err);
        }
    }

    if profiling::enabled() {
        timer.log_with(&format!("ok={} {}", result.is_ok(), metric_suffix));
    } else {
        timer.log();
    }

    result
}

/// テキスト入力ワーカーを初期化
pub fn init_worker() -> Result<(), TextInputWorkerError> {
    if worker_slot()
        .lock()
        .map_err(|e| TextInputWorkerError::ChannelClosed(format!("worker lock poisoned: {}", e)))?
        .is_some()
    {
        return Ok(());
    }

    let _ = replace_worker_handle()?;
    Ok(())
}

/// スリープ復帰後にワーカーを張り直す
pub fn recover_after_wake() -> Result<(), TextInputWorkerError> {
    let _ = worker_slot()
        .lock()
        .map_err(|e| TextInputWorkerError::ChannelClosed(format!("worker lock poisoned: {}", e)))?
        .take();
    let _ = replace_worker_handle()?;
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
/// # async fn main() -> Result<(), voice_input::infrastructure::external::text_input_worker::TextInputWorkerError> {
/// type_text("Hello, World!").await?;
/// # Ok(())
/// # }
/// ```
pub async fn type_text(text: &str) -> Result<(), TextInputWorkerError> {
    run_with_recovery(
        "text_input.worker",
        format!("text_len={}", text.len()),
        |handle| async move { handle.type_text(text).await },
    )
    .await
}

/// 連続入力の一部としてテキストを入力する
pub async fn type_text_continuous(text: &str) -> Result<(), TextInputWorkerError> {
    run_with_recovery(
        "text_input.worker_continuous",
        format!("text_len={}", text.len()),
        |handle| async move { handle.type_text_continuous(text).await },
    )
    .await
}

/// 入力済みテキストの末尾差分を置き換える
pub async fn replace_suffix(delete_count: usize, text: &str) -> Result<(), TextInputWorkerError> {
    run_with_recovery(
        "text_input.worker_replace",
        format!("delete_count={} text_len={}", delete_count, text.len()),
        |handle| async move { handle.replace_suffix(delete_count, text).await },
    )
    .await
}

/// 連続入力の一部として入力済みテキストの末尾差分を置き換える
pub async fn replace_suffix_continuous(
    delete_count: usize,
    text: &str,
) -> Result<(), TextInputWorkerError> {
    run_with_recovery(
        "text_input.worker_replace_continuous",
        format!("delete_count={} text_len={}", delete_count, text.len()),
        |handle| async move { handle.replace_suffix_continuous(delete_count, text).await },
    )
    .await
}

/// 直近に入力したテキスト範囲を相対位置で選択する
pub async fn select_recent_range(
    trailing_char_count: usize,
    char_count: usize,
) -> Result<(), TextInputWorkerError> {
    run_with_recovery(
        "text_input.worker_select_recent_range",
        format!(
            "trailing_char_count={} char_count={}",
            trailing_char_count, char_count
        ),
        |handle| async move {
            handle
                .select_recent_range(trailing_char_count, char_count)
                .await
        },
    )
    .await
}
