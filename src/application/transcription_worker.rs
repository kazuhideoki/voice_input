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
use async_trait::async_trait;

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
            process_streaming_events(&mut event_rx, &ProfiledTextApplier).await;
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

async fn type_text_continuous_with_profile(text: &str) {
    let input_timer = profiling::Timer::start("text_input.continuous");
    match text_input::type_text_continuous(text).await {
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
            eprintln!("Direct input continuous failed: {}", e);
        }
    }
}

async fn apply_text_patch_continuous_with_profile(current: &str, next: &str) {
    let (delete_count, append_text) = diff_text_for_patch(current, next);
    if delete_count == 0 && append_text.is_empty() {
        return;
    }

    let input_timer = profiling::Timer::start("text_input.patch_continuous");
    match text_input::replace_suffix_continuous(delete_count, &append_text).await {
        Ok(_) => {
            if profiling::enabled() {
                input_timer.log_with(&format!(
                    "ok=true delete_count={} text_len={}",
                    delete_count,
                    append_text.len()
                ));
            } else {
                input_timer.log();
            }
        }
        Err(e) => {
            if profiling::enabled() {
                input_timer.log_with(&format!(
                    "ok=false delete_count={} text_len={}",
                    delete_count,
                    append_text.len()
                ));
            } else {
                input_timer.log();
            }
            eprintln!("Direct input continuous patch failed: {}", e);
        }
    }
}

fn diff_text_for_patch(current: &str, next: &str) -> (usize, String) {
    let prefix_bytes = current
        .chars()
        .zip(next.chars())
        .take_while(|(lhs, rhs)| lhs == rhs)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();

    let delete_count = current[prefix_bytes..].chars().count();
    let append_text = next[prefix_bytes..].to_string();

    (delete_count, append_text)
}

#[async_trait(?Send)]
trait TextApplier {
    async fn type_text(&self, text: &str);
    async fn type_text_continuous(&self, text: &str);
    async fn patch_text_continuous(&self, current: &str, next: &str);
}

struct ProfiledTextApplier;

#[async_trait(?Send)]
impl TextApplier for ProfiledTextApplier {
    async fn type_text(&self, text: &str) {
        type_text_with_profile(text).await;
    }

    async fn type_text_continuous(&self, text: &str) {
        type_text_continuous_with_profile(text).await;
    }

    async fn patch_text_continuous(&self, current: &str, next: &str) {
        apply_text_patch_continuous_with_profile(current, next).await;
    }
}

async fn process_streaming_events(
    event_rx: &mut tokio::sync::mpsc::UnboundedReceiver<TranscriptionEvent>,
    text_applier: &dyn TextApplier,
) {
    let mut rendered_text = String::new();
    while let Some(event) = event_rx.recv().await {
        match event {
            TranscriptionEvent::Delta(delta) => {
                if rendered_text.is_empty() {
                    text_applier.type_text(&delta).await;
                } else {
                    text_applier.type_text_continuous(&delta).await;
                }
                rendered_text.push_str(&delta);
            }
            TranscriptionEvent::Completed(text) if rendered_text.is_empty() => {
                text_applier.type_text(&text).await;
                break;
            }
            TranscriptionEvent::Completed(text) => {
                text_applier
                    .patch_text_continuous(&rendered_text, &text)
                    .await;
                break;
            }
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
            if let Err(e) = handle_transcription(result, resume_music, transcription_service).await
            {
                eprintln!("Transcription handling failed: {}", e);
            }
            drop(permit);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{TextApplier, diff_text_for_patch, process_streaming_events};
    use crate::application::TranscriptionEvent;
    use async_trait::async_trait;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// 末尾追記だけなら削除せず差分だけ追加する
    #[test]
    fn diff_text_for_patch_appends_suffix_without_deleting() {
        let (delete_count, append_text) = diff_text_for_patch("こん", "こんにちは");

        assert_eq!(delete_count, 0);
        assert_eq!(append_text, "にちは");
    }

    /// 中間が変わる場合は共通接頭辞以降を削除して再入力する
    #[test]
    fn diff_text_for_patch_replaces_suffix_after_common_prefix() {
        let (delete_count, append_text) = diff_text_for_patch("これはテストです", "これはtestです");

        assert_eq!(delete_count, 5);
        assert_eq!(append_text, "testです");
    }

    /// Deltaの後にCompletedが来たら差分置き換えで最終文字列へ補正する
    #[tokio::test]
    async fn streaming_events_use_replace_suffix_after_delta_input() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        struct MockTextApplier {
            calls: Rc<RefCell<Vec<String>>>,
            completed: Rc<Cell<bool>>,
        }

        #[async_trait(?Send)]
        impl TextApplier for MockTextApplier {
            async fn type_text(&self, text: &str) {
                self.calls.borrow_mut().push(format!("type:{text}"));
            }

            async fn type_text_continuous(&self, text: &str) {
                self.calls
                    .borrow_mut()
                    .push(format!("type_continuous:{text}"));
            }

            async fn patch_text_continuous(&self, current: &str, next: &str) {
                self.calls
                    .borrow_mut()
                    .push(format!("patch_continuous:{current}->{next}"));
                self.completed.set(true);
            }
        }

        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        let completed = Rc::new(Cell::new(false));
        let text_applier = MockTextApplier {
            calls: calls.clone(),
            completed: completed.clone(),
        };

        event_tx
            .send(TranscriptionEvent::Delta("これは".to_string()))
            .unwrap();
        event_tx
            .send(TranscriptionEvent::Delta("テストです".to_string()))
            .unwrap();
        event_tx
            .send(TranscriptionEvent::Completed("これはtestです".to_string()))
            .unwrap();
        drop(event_tx);

        process_streaming_events(&mut event_rx, &text_applier).await;

        assert_eq!(
            *calls.borrow(),
            vec![
                "type:これは".to_string(),
                "type_continuous:テストです".to_string(),
                "patch_continuous:これはテストです->これはtestです".to_string()
            ]
        );
        assert!(completed.get());
    }
}
