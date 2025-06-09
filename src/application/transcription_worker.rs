//! 転写ワーカー
//!
//! # 責任
//! - 録音結果の転写処理
//! - 辞書変換の適用
//! - スタックへの保存
//! - ペースト処理

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::application::{
    StackService, TranscriptionMessage, TranscriptionOptions, TranscriptionService,
};
use crate::error::Result;
use crate::infrastructure::{
    external::{sound::resume_apple_music, text_input},
    ui::{StackDisplayInfo, UiNotification, UiProcessManager},
};
use crate::ipc::RecordingResult;

/// 転写結果を処理
pub async fn handle_transcription(
    result: RecordingResult,
    paste: bool,
    resume_music: bool,
    direct_input: bool,
    stack_service: Option<Rc<RefCell<StackService>>>,
    ui_manager: Option<Rc<RefCell<UiProcessManager>>>,
    transcription_service: Rc<RefCell<TranscriptionService>>,
) -> Result<()> {
    // エラーが発生しても確実に音楽を再開するためにdeferパターンで実装
    let _defer_guard = scopeguard::guard(resume_music, |should_resume| {
        if should_resume {
            // 念のため少し遅延を入れて他の処理が完了するのを待つ
            std::thread::sleep(std::time::Duration::from_millis(100));
            resume_apple_music();
        }
    });

    // 転写オプションを構築
    let options = TranscriptionOptions {
        language: "ja".to_string(),
        prompt: None, // メモリモードではプロンプトファイルを使用しない
    };

    // 転写実行
    let text = transcription_service
        .borrow()
        .transcribe(result.audio_data.into(), options)
        .await?;

    // スタックモードが有効な場合は自動保存
    if let Some(stack_service_ref) = &stack_service {
        if stack_service_ref.borrow().is_stack_mode_enabled() {
            let stack_id = stack_service_ref.borrow_mut().save_stack(text.clone());
            let preview = text.chars().take(30).collect::<String>();
            println!(
                "{}",
                crate::application::UserFeedback::stack_saved(stack_id, &preview)
            );

            // UI にスタック追加を通知
            if let Some(ui_manager_ref) = &ui_manager {
                if let Ok(manager) = ui_manager_ref.try_borrow() {
                    let stack_info = StackDisplayInfo {
                        number: stack_id,
                        preview: preview.clone(),
                        created_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                            .to_string(),
                        is_active: false,
                        char_count: text.len(),
                    };
                    let _ = manager.notify(UiNotification::StackAdded(stack_info));
                }
            }
        }
    }

    // スタックモードが有効な場合は自動ペーストを無効化
    let should_paste = paste
        && (stack_service.is_none()
            || !stack_service
                .as_ref()
                .unwrap()
                .borrow()
                .is_stack_mode_enabled());

    // 即貼り付け
    if should_paste {
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

        if direct_input {
            // 直接入力方式（日本語対応）
            match text_input::type_text(&text).await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Direct input failed: {}", e);
                    // フォールバック処理は削除（クリップボードを汚染しないため）
                }
            }
        } else {
            // direct_inputでない場合は何もしない（クリップボードを汚染しないため）
            eprintln!("Paste mode without direct_input is no longer supported");
        }
    }

    Ok(())
}

/// 転写ワーカーを起動
pub async fn spawn_transcription_worker(
    semaphore: Arc<Semaphore>,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TranscriptionMessage>,
    transcription_service: Rc<RefCell<TranscriptionService>>,
) {
    use tokio::task::spawn_local;

    while let Some((result, paste, resume_music, direct_input, stack_service, ui_manager)) =
        rx.recv().await
    {
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("semaphore acquire error: {}", e);
                continue;
            }
        };

        let transcription_service = transcription_service.clone();
        spawn_local(async move {
            let _ = handle_transcription(
                result,
                paste,
                resume_music,
                direct_input,
                stack_service,
                ui_manager,
                transcription_service,
            )
            .await;
            drop(permit);
        });
    }
}
