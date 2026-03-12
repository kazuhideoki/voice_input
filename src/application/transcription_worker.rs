//! 転写ワーカー
//!
//! # 責任
//! - 録音結果の転写処理
//! - 辞書変換の適用
//! - 直接入力処理

#![allow(clippy::await_holding_refcell_ref)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::application::{
    TranscriptionEvent, TranscriptionMessage, TranscriptionOptions, TranscriptionService,
};
use crate::error::Result;
use crate::infrastructure::external::{sound::resume_apple_music, text_input};
use crate::ipc::RecordingResult;
use crate::utils::config::EnvConfig;
use crate::utils::profiling;

/// 転写結果を処理
pub async fn handle_transcription(
    result: RecordingResult,
    resume_music: bool,
    transcription_service: Rc<RefCell<TranscriptionService>>,
) -> Result<()> {
    let overall_timer = profiling::Timer::start("transcription.handle");

    // エラーが発生しても確実に音楽を再開するためにdeferパターンで実装
    let _defer_guard = scopeguard::guard(resume_music, |should_resume| {
        if should_resume {
            tokio::task::spawn_local(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                resume_apple_music();
            });
        }
    });

    // 転写オプションを構築
    let options = TranscriptionOptions {
        language: "ja".to_string(),
        prompt: None, // メモリモードではプロンプトファイルを使用しない
    };

    let text = if EnvConfig::get().openai_transcribe_streaming {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let input_task = tokio::task::spawn_local(async move {
            let mut delta_emitted = false;
            while let Some(event) = event_rx.recv().await {
                match event {
                    TranscriptionEvent::Delta(delta) => {
                        delta_emitted = true;
                        type_text_with_profile(&delta).await;
                    }
                    TranscriptionEvent::Completed(text) if !delta_emitted => {
                        type_text_with_profile(&text).await;
                    }
                    TranscriptionEvent::Completed(_) => break,
                }
            }
        });

        let text = transcription_service
            .borrow()
            .transcribe_streaming(result.audio_data.into(), options, event_tx)
            .await?;

        match input_task.await {
            Ok(()) => {}
            Err(e) => eprintln!("Streaming input task failed: {}", e),
        }

        text
    } else {
        let text = transcription_service
            .borrow()
            .transcribe(result.audio_data.into(), options)
            .await?;
        type_text_with_profile(&text).await;
        text
    };

    if profiling::enabled() {
        overall_timer.log_with(&format!("text_len={}", text.len()));
    } else {
        overall_timer.log();
    }

    Ok(())
}

async fn type_text_with_profile(text: &str) {
    tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;
    let input_timer = profiling::Timer::start("text_input");
    match text_input::type_text(text).await {
        Ok(_) => {
            if profiling::enabled() {
                input_timer.log_with(&format!("ok=true text_len={}", text.len()));
            } else {
                input_timer.log();
            }
        }
        Err(e) => {
            if profiling::enabled() {
                input_timer.log_with(&format!("ok=false text_len={}", text.len()));
            } else {
                input_timer.log();
            }
            eprintln!("Direct input failed: {}", e);
        }
    }
}

/// 転写ワーカーを起動
pub async fn spawn_transcription_worker(
    semaphore: Arc<Semaphore>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TranscriptionMessage>,
    transcription_service: Rc<RefCell<TranscriptionService>>,
) {
    use tokio::task::spawn_local;

    while let Some((result, resume_music)) = rx.recv().await {
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("semaphore acquire error: {}", e);
                continue;
            }
        };

        let transcription_service = transcription_service.clone();
        spawn_local(async move {
            let _ = handle_transcription(result, resume_music, transcription_service).await;
            drop(permit);
        });
    }
}
