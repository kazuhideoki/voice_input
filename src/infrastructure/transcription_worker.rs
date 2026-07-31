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

use crate::application::AudioBackend;
use crate::application::{
    RecordingService, TranscriptionEvent, TranscriptionHistoryService, TranscriptionOptions,
    TranscriptionService,
};
use crate::domain::transcription::FinalizedTranscription;
use crate::error::Result;
use crate::infrastructure::command_handler::TranscriptionMessage;
use crate::infrastructure::config::AppConfig;
use crate::infrastructure::external::{
    recording_hud::{self, HudState},
    sound::resume_apple_music,
    text_input,
};
use crate::utils::config::TranscriptionProvider;
use crate::utils::profiling;
use async_trait::async_trait;

/// 転写結果を処理
pub async fn handle_transcription(
    message: TranscriptionMessage,
    transcription_service: Rc<RefCell<TranscriptionService>>,
    history_service: Rc<RefCell<TranscriptionHistoryService>>,
) -> Result<()> {
    let overall_timer = profiling::Timer::start("transcription.handle");
    let TranscriptionMessage {
        result,
        resume_music,
        session_id: _,
        provider,
    } = message;

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
    let options = TranscriptionOptions { provider };

    let finalized = if provider == TranscriptionProvider::GptTranscribe
        && AppConfig::load_runtime().effective_transcribe_streaming()
    {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let input_task = tokio::task::spawn_local(async move {
            process_streaming_events(&mut event_rx, &ProfiledTextApplier).await
        });

        let finalized = transcription_service
            .borrow()
            .transcribe_streaming(result.audio_data, options, event_tx)
            .await?;

        match input_task.await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Streaming input task failed: {}", e);
            }
        }

        finalized
    } else {
        let finalized = transcription_service
            .borrow()
            .transcribe(result.audio_data, options)
            .await?;
        type_text_with_profile(&finalized.text).await;
        finalized
    };

    if profiling::enabled() {
        overall_timer.log_with(&format!("text_len={}", finalized.text.len()));
    } else {
        overall_timer.log();
    }
    history_service
        .borrow_mut()
        .record_finalized(&finalized, provider);

    Ok(())
}

async fn type_text_with_profile(text: &str) -> bool {
    let input_timer = profiling::Timer::start("text_input");
    match text_input::type_text(text).await {
        Ok(_) => {
            if profiling::enabled() {
                input_timer.log_with(&format!("ok=true text_len={}", text.len()));
            } else {
                input_timer.log();
            }
            true
        }
        Err(e) => {
            if profiling::enabled() {
                input_timer.log_with(&format!("ok=false text_len={}", text.len()));
            } else {
                input_timer.log();
            }
            eprintln!("Direct input failed: {}", e);
            false
        }
    }
}

async fn type_text_continuous_with_profile(text: &str) -> bool {
    let input_timer = profiling::Timer::start("text_input.continuous");
    match text_input::type_text_continuous(text).await {
        Ok(_) => {
            if profiling::enabled() {
                input_timer.log_with(&format!("ok=true text_len={}", text.len()));
            } else {
                input_timer.log();
            }
            true
        }
        Err(e) => {
            if profiling::enabled() {
                input_timer.log_with(&format!("ok=false text_len={}", text.len()));
            } else {
                input_timer.log();
            }
            eprintln!("Direct input continuous failed: {}", e);
            false
        }
    }
}

async fn apply_text_patch_continuous_with_profile(current: &str, next: &str) -> bool {
    let (delete_count, append_text) = diff_text_for_patch(current, next);
    if delete_count == 0 && append_text.is_empty() {
        return true;
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
            true
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
            false
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
    async fn type_text(&self, text: &str) -> bool;
    async fn type_text_continuous(&self, text: &str) -> bool;
    async fn patch_text_continuous(&self, current: &str, next: &str) -> bool;
}

struct ProfiledTextApplier;

#[async_trait(?Send)]
impl TextApplier for ProfiledTextApplier {
    async fn type_text(&self, text: &str) -> bool {
        type_text_with_profile(text).await
    }

    async fn type_text_continuous(&self, text: &str) -> bool {
        type_text_continuous_with_profile(text).await
    }

    async fn patch_text_continuous(&self, current: &str, next: &str) -> bool {
        apply_text_patch_continuous_with_profile(current, next).await
    }
}

pub(crate) async fn process_streaming_text_input(
    event_rx: &mut tokio::sync::mpsc::UnboundedReceiver<TranscriptionEvent>,
) -> Option<FinalizedTranscription> {
    process_streaming_events(event_rx, &ProfiledTextApplier).await
}

async fn process_streaming_events(
    event_rx: &mut tokio::sync::mpsc::UnboundedReceiver<TranscriptionEvent>,
    text_applier: &dyn TextApplier,
) -> Option<FinalizedTranscription> {
    let mut rendered_text = String::new();
    let mut input_succeeded = true;
    while let Some(event) = event_rx.recv().await {
        match event {
            TranscriptionEvent::Delta(delta) => {
                let delta = if rendered_text.is_empty() {
                    delta.trim_start()
                } else {
                    delta.as_str()
                };
                if delta.is_empty() {
                    continue;
                }
                if input_succeeded {
                    if rendered_text.is_empty() {
                        input_succeeded = text_applier.type_text(delta).await;
                    } else {
                        input_succeeded = text_applier.type_text_continuous(delta).await;
                    }
                }
                rendered_text.push_str(delta);
            }
            TranscriptionEvent::Completed(finalized) if rendered_text.is_empty() => {
                if input_succeeded {
                    text_applier.type_text(&finalized.text).await;
                }
                return Some(finalized);
            }
            TranscriptionEvent::Completed(finalized) => {
                // 一度でも direct input に失敗したら、入力先との同期は崩れたとみなす。
                // その状態で後続 delta や最終 patch を送り続けると、既存テキスト破壊の
                // 可能性があるため、以後はイベントを受け流すだけにして副作用を止める。
                if input_succeeded {
                    text_applier
                        .patch_text_continuous(&rendered_text, &finalized.text)
                        .await;
                }
                return Some(finalized);
            }
        }
    }

    None
}

/// 転写ワーカーを起動
pub async fn spawn_transcription_worker<T: AudioBackend + 'static>(
    semaphore: Arc<Semaphore>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TranscriptionMessage>,
    transcription_service: Rc<RefCell<TranscriptionService>>,
    history_service: Rc<RefCell<TranscriptionHistoryService>>,
    recording_service: Rc<RefCell<RecordingService<T>>>,
) {
    use tokio::task::spawn_local;

    while let Some(message) = rx.recv().await {
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("semaphore acquire error: {}", e);
                continue;
            }
        };

        let transcription_service = transcription_service.clone();
        let history_service = history_service.clone();
        let recording_service = recording_service.clone();
        spawn_local(async move {
            let session_id = message.session_id;
            if let Err(e) =
                handle_transcription(message, transcription_service, history_service).await
            {
                eprintln!("Transcription handling failed: {}", e);
            }
            hide_hud_if_no_newer_session(session_id, recording_service);
            drop(permit);
        });
    }
}

fn hide_hud_if_no_newer_session<T: AudioBackend>(
    session_id: u64,
    recording_service: Rc<RefCell<RecordingService<T>>>,
) {
    let should_hide = match recording_service
        .borrow()
        .has_started_newer_session(session_id)
    {
        Ok(value) => !value,
        Err(e) => {
            eprintln!("Failed to check newer session before hiding HUD: {}", e);
            true
        }
    };

    if should_hide {
        recording_hud::set_state(HudState::Hidden);
    }
}

#[cfg(test)]
mod tests {
    use super::{TextApplier, diff_text_for_patch, process_streaming_events};
    use crate::application::TranscriptionEvent;
    use crate::domain::transcription::FinalizedTranscription;
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
            async fn type_text(&self, text: &str) -> bool {
                self.calls.borrow_mut().push(format!("type:{text}"));
                true
            }

            async fn type_text_continuous(&self, text: &str) -> bool {
                self.calls
                    .borrow_mut()
                    .push(format!("type_continuous:{text}"));
                true
            }

            async fn patch_text_continuous(&self, current: &str, next: &str) -> bool {
                self.calls
                    .borrow_mut()
                    .push(format!("patch_continuous:{current}->{next}"));
                self.completed.set(true);
                true
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
            .send(TranscriptionEvent::Completed(FinalizedTranscription {
                text: "これはtestです".to_string(),
            }))
            .unwrap();
        drop(event_tx);

        let finalized = process_streaming_events(&mut event_rx, &text_applier).await;

        assert_eq!(
            *calls.borrow(),
            vec![
                "type:これは".to_string(),
                "type_continuous:テストです".to_string(),
                "patch_continuous:これはテストです->これはtestです".to_string()
            ]
        );
        assert!(completed.get());
        assert_eq!(
            finalized,
            Some(FinalizedTranscription {
                text: "これはtestです".to_string(),
            })
        );
    }

    /// 初回Deltaの先頭空白は途中入力に反映しない
    #[tokio::test]
    async fn streaming_events_trim_leading_space_from_first_delta_input() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        struct MockTextApplier {
            calls: Rc<RefCell<Vec<String>>>,
        }

        #[async_trait(?Send)]
        impl TextApplier for MockTextApplier {
            async fn type_text(&self, text: &str) -> bool {
                self.calls.borrow_mut().push(format!("type:{text}"));
                true
            }

            async fn type_text_continuous(&self, text: &str) -> bool {
                self.calls
                    .borrow_mut()
                    .push(format!("type_continuous:{text}"));
                true
            }

            async fn patch_text_continuous(&self, current: &str, next: &str) -> bool {
                self.calls
                    .borrow_mut()
                    .push(format!("patch_continuous:{current}->{next}"));
                true
            }
        }

        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        let text_applier = MockTextApplier {
            calls: calls.clone(),
        };

        event_tx
            .send(TranscriptionEvent::Delta(" これは".to_string()))
            .unwrap();
        event_tx
            .send(TranscriptionEvent::Delta("テストです".to_string()))
            .unwrap();
        event_tx
            .send(TranscriptionEvent::Completed(FinalizedTranscription {
                text: "これはテストです".to_string(),
            }))
            .unwrap();
        drop(event_tx);

        let finalized = process_streaming_events(&mut event_rx, &text_applier).await;

        assert_eq!(
            *calls.borrow(),
            vec![
                "type:これは".to_string(),
                "type_continuous:テストです".to_string(),
                "patch_continuous:これはテストです->これはテストです".to_string()
            ]
        );
        assert_eq!(
            finalized,
            Some(FinalizedTranscription {
                text: "これはテストです".to_string(),
            })
        );
    }

    /// 空白だけの初回Deltaは入力せず後続Deltaを初回入力として扱う
    #[tokio::test]
    async fn streaming_events_skip_initial_whitespace_only_delta() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        struct MockTextApplier {
            calls: Rc<RefCell<Vec<String>>>,
        }

        #[async_trait(?Send)]
        impl TextApplier for MockTextApplier {
            async fn type_text(&self, text: &str) -> bool {
                self.calls.borrow_mut().push(format!("type:{text}"));
                true
            }

            async fn type_text_continuous(&self, text: &str) -> bool {
                self.calls
                    .borrow_mut()
                    .push(format!("type_continuous:{text}"));
                true
            }

            async fn patch_text_continuous(&self, current: &str, next: &str) -> bool {
                self.calls
                    .borrow_mut()
                    .push(format!("patch_continuous:{current}->{next}"));
                true
            }
        }

        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        let text_applier = MockTextApplier {
            calls: calls.clone(),
        };

        event_tx
            .send(TranscriptionEvent::Delta(" ".to_string()))
            .unwrap();
        event_tx
            .send(TranscriptionEvent::Delta("これは".to_string()))
            .unwrap();
        event_tx
            .send(TranscriptionEvent::Completed(FinalizedTranscription {
                text: "これは".to_string(),
            }))
            .unwrap();
        drop(event_tx);

        let finalized = process_streaming_events(&mut event_rx, &text_applier).await;

        assert_eq!(
            *calls.borrow(),
            vec![
                "type:これは".to_string(),
                "patch_continuous:これは->これは".to_string()
            ]
        );
        assert_eq!(
            finalized,
            Some(FinalizedTranscription {
                text: "これは".to_string(),
            })
        );
    }

    /// ストリーミング入力が失敗しても確定結果は返す
    #[tokio::test]
    async fn streaming_events_return_finalized_text_after_input_failure() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

        struct FailingTextApplier;

        #[async_trait(?Send)]
        impl TextApplier for FailingTextApplier {
            async fn type_text(&self, _text: &str) -> bool {
                false
            }

            async fn type_text_continuous(&self, _text: &str) -> bool {
                false
            }

            async fn patch_text_continuous(&self, _current: &str, _next: &str) -> bool {
                false
            }
        }

        event_tx
            .send(TranscriptionEvent::Completed(FinalizedTranscription {
                text: "失敗".to_string(),
            }))
            .unwrap();
        drop(event_tx);

        let finalized = process_streaming_events(&mut event_rx, &FailingTextApplier).await;

        assert_eq!(
            finalized,
            Some(FinalizedTranscription {
                text: "失敗".to_string(),
            })
        );
    }

    /// ストリーミング入力が途中で失敗したら以後の入力副作用を止める
    #[tokio::test]
    async fn streaming_events_stop_side_effects_after_first_input_failure() {
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

        struct DesyncingTextApplier {
            calls: Rc<RefCell<Vec<String>>>,
        }

        #[async_trait(?Send)]
        impl TextApplier for DesyncingTextApplier {
            async fn type_text(&self, text: &str) -> bool {
                self.calls.borrow_mut().push(format!("type:{text}"));
                false
            }

            async fn type_text_continuous(&self, text: &str) -> bool {
                self.calls
                    .borrow_mut()
                    .push(format!("type_continuous:{text}"));
                true
            }

            async fn patch_text_continuous(&self, current: &str, next: &str) -> bool {
                self.calls
                    .borrow_mut()
                    .push(format!("patch_continuous:{current}->{next}"));
                true
            }
        }

        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        let text_applier = DesyncingTextApplier {
            calls: calls.clone(),
        };

        event_tx
            .send(TranscriptionEvent::Delta("これは".to_string()))
            .unwrap();
        event_tx
            .send(TranscriptionEvent::Delta("テストです".to_string()))
            .unwrap();
        event_tx
            .send(TranscriptionEvent::Completed(FinalizedTranscription {
                text: "これはtestです".to_string(),
            }))
            .unwrap();
        drop(event_tx);

        let finalized = process_streaming_events(&mut event_rx, &text_applier).await;

        assert_eq!(*calls.borrow(), vec!["type:これは".to_string()]);
        assert_eq!(
            finalized,
            Some(FinalizedTranscription {
                text: "これはtestです".to_string(),
            })
        );
    }
}
