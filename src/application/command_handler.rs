//! IPCコマンドハンドラー
//!
//! # 責任
//! - IPCコマンドの処理と適切なサービスへの委譲
//! - サービス間の調整
//! - レスポンスの生成

#![allow(clippy::await_holding_refcell_ref)]

use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;
use tokio::task::spawn_local;
use tokio::time::Duration;

use crate::application::{
    MediaControlService, RecordingOptions, RecordingService, StackService, TranscriptionService,
    UserFeedback,
};
use crate::error::{Result, VoiceInputError};
use crate::infrastructure::{
    audio::{AudioBackend, CpalAudioBackend},
    external::{
        sound::{play_start_sound, play_stop_sound},
        text_input,
    },
    ui::{UiNotification, UiProcessManager},
};
use crate::ipc::{IpcCmd, IpcResp, RecordingResult};
use crate::shortcut::ShortcutService;

/// 転写メッセージ
pub type TranscriptionMessage = (
    RecordingResult,
    bool, // paste
    bool, // resume_music
    bool, // direct_input
    Option<Rc<RefCell<StackService>>>,
    Option<Rc<RefCell<UiProcessManager>>>,
);

/// コマンドハンドラー
pub struct CommandHandler<T: AudioBackend> {
    recording: Rc<RefCell<RecordingService<T>>>,
    #[allow(dead_code)]
    transcription: Rc<RefCell<TranscriptionService>>,
    stack: Rc<RefCell<StackService>>,
    media_control: Rc<RefCell<MediaControlService>>,
    ui_manager: Rc<RefCell<UiProcessManager>>,
    shortcut_service: Rc<RefCell<ShortcutService>>,
    transcription_tx: mpsc::UnboundedSender<TranscriptionMessage>,
}

impl<T: AudioBackend + 'static> CommandHandler<T> {
    /// 新しいCommandHandlerを作成
    pub fn new(
        recording: Rc<RefCell<RecordingService<T>>>,
        transcription: Rc<RefCell<TranscriptionService>>,
        stack: Rc<RefCell<StackService>>,
        media_control: Rc<RefCell<MediaControlService>>,
        ui_manager: Rc<RefCell<UiProcessManager>>,
        shortcut_service: Rc<RefCell<ShortcutService>>,
        transcription_tx: mpsc::UnboundedSender<TranscriptionMessage>,
    ) -> Self {
        Self {
            recording,
            transcription,
            stack,
            media_control,
            ui_manager,
            shortcut_service,
            transcription_tx,
        }
    }

    /// IPCコマンドを処理
    pub async fn handle(&self, cmd: IpcCmd) -> Result<IpcResp> {
        match cmd {
            IpcCmd::Start {
                paste,
                prompt,
                direct_input,
            } => self.handle_start(paste, prompt, direct_input).await,
            IpcCmd::Stop => self.handle_stop().await,
            IpcCmd::Toggle {
                paste,
                prompt,
                direct_input,
            } => {
                if self.recording.borrow().is_recording() {
                    self.handle_stop().await
                } else {
                    self.handle_start(paste, prompt, direct_input).await
                }
            }
            IpcCmd::Status => self.handle_status(),
            IpcCmd::ListDevices => self.handle_list_devices(),
            IpcCmd::Health => self.handle_health().await,
            IpcCmd::EnableStackMode => self.handle_enable_stack_mode().await,
            IpcCmd::DisableStackMode => self.handle_disable_stack_mode().await,
            IpcCmd::PasteStack { number } => self.handle_paste_stack(number).await,
            IpcCmd::ListStacks => self.handle_list_stacks(),
            IpcCmd::ClearStacks => self.handle_clear_stacks(),
        }
    }

    /// 録音開始処理
    async fn handle_start(
        &self,
        paste: bool,
        prompt: Option<String>,
        direct_input: bool,
    ) -> Result<IpcResp> {
        // Apple Musicを一時停止
        let media_control = self.media_control.clone();
        let was_playing = media_control.borrow().pause_if_playing().await?;
        self.recording.borrow().set_music_was_playing(was_playing)?;

        // 開始音を再生
        play_start_sound();

        // 録音オプションを構築
        let options = RecordingOptions {
            prompt,
            paste,
            direct_input,
        };

        // 録音を開始
        let recording = self.recording.clone();
        let _session_id = recording.borrow().start_recording(options).await?;

        // 自動停止タイマーを設定
        self.setup_auto_stop_timer();

        let max_secs = self.recording.borrow().config().max_duration_secs;
        Ok(IpcResp {
            ok: true,
            msg: format!("recording started (auto-stop in {}s)", max_secs),
        })
    }

    /// 録音停止処理
    async fn handle_stop(&self) -> Result<IpcResp> {
        // 停止音を再生
        play_stop_sound();

        // 録音を停止
        let recording = self.recording.clone();
        let result = recording.borrow().stop_recording().await?;

        // コンテキスト情報を取得
        let (_start_prompt, paste, direct_input, music_was_playing) =
            self.recording.borrow().get_context_info()?;

        // スタックモードが有効な場合はサービスを渡す
        let stack_for_transcription = if self.stack.borrow().is_stack_mode_enabled() {
            Some(self.stack.clone())
        } else {
            None
        };

        // 転写キューに送信
        self.transcription_tx
            .send((
                result,
                paste,
                music_was_playing,
                direct_input,
                stack_for_transcription,
                Some(self.ui_manager.clone()),
            ))
            .map_err(|e| {
                VoiceInputError::SystemError(format!(
                    "Failed to send to transcription queue: {}",
                    e
                ))
            })?;

        Ok(IpcResp {
            ok: true,
            msg: "recording stopped; queued".to_string(),
        })
    }

    /// ステータス取得
    fn handle_status(&self) -> Result<IpcResp> {
        let state = if self.recording.borrow().is_recording() {
            "Recording"
        } else {
            "Idle"
        };

        Ok(IpcResp {
            ok: true,
            msg: format!("state={}", state),
        })
    }

    /// デバイス一覧取得
    fn handle_list_devices(&self) -> Result<IpcResp> {
        let devices = CpalAudioBackend::list_devices();
        Ok(IpcResp {
            ok: true,
            msg: if devices.is_empty() {
                "⚠️  No input devices detected".to_string()
            } else {
                devices.join("\n")
            },
        })
    }

    /// ヘルスチェック
    async fn handle_health(&self) -> Result<IpcResp> {
        let mut ok = true;
        let mut lines = Vec::new();

        // デバイスチェック
        if CpalAudioBackend::list_devices().is_empty() {
            lines.push("Input device: MISSING".to_string());
            ok = false;
        } else {
            lines.push("Input device: OK".to_string());
        }

        // OpenAI APIチェック
        match std::env::var("OPENAI_API_KEY") {
            Ok(key) => {
                lines.push("OPENAI_API_KEY: present".to_string());
                let client = reqwest::Client::new();
                match client
                    .get("https://api.openai.com/v1/models")
                    .bearer_auth(key)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        lines.push("OpenAI API: reachable".to_string());
                    }
                    Ok(resp) => {
                        lines.push(format!("OpenAI API: fail({})", resp.status()));
                        ok = false;
                    }
                    Err(e) => {
                        lines.push(format!("OpenAI API: error({})", e));
                        ok = false;
                    }
                }
            }
            Err(_) => {
                lines.push("OPENAI_API_KEY: missing".to_string());
                ok = false;
            }
        }

        Ok(IpcResp {
            ok,
            msg: lines.join("\n"),
        })
    }

    /// スタックモード有効化
    async fn handle_enable_stack_mode(&self) -> Result<IpcResp> {
        let count = {
            let mut service = self.stack.borrow_mut();
            service.enable_stack_mode();
            service.list_stacks().len()
        };

        // UI起動を試行
        let ui_manager = self.ui_manager.clone();
        if let Ok(mut manager) = ui_manager.try_borrow_mut() {
            if let Err(e) = manager.start_ui().await {
                eprintln!("UI process start failed (continuing without UI): {}", e);
            } else {
                drop(manager); // borrowを解放
                // UI起動後に状態変更を通知
                tokio::time::sleep(Duration::from_millis(100)).await;
                if let Ok(manager) = ui_manager.try_borrow() {
                    let _ = manager.notify(UiNotification::ModeChanged(true));
                }
            }
        }

        // ショートカットサービスの起動を試行
        if !self.shortcut_service.borrow().is_enabled() {
            println!("Starting shortcut service with stack mode...");
            // 注: 実際のショートカット起動はvoice_inputd.rsで行う（IPCチャンネルが必要なため）
        }

        Ok(IpcResp {
            ok: true,
            msg: UserFeedback::mode_status(true, count),
        })
    }

    /// スタックモード無効化
    async fn handle_disable_stack_mode(&self) -> Result<IpcResp> {
        self.stack.borrow_mut().disable_stack_mode();

        // UI停止
        if let Ok(mut manager) = self.ui_manager.try_borrow_mut() {
            let _ = manager.notify(UiNotification::ModeChanged(false));
            if let Err(e) = manager.stop_ui() {
                eprintln!("UI process stop failed: {}", e);
            }
        }

        Ok(IpcResp {
            ok: true,
            msg: UserFeedback::mode_status(false, 0),
        })
    }

    /// スタックペースト
    async fn handle_paste_stack(&self, number: u32) -> Result<IpcResp> {
        let (stack_text, char_count) = {
            let service = self.stack.borrow();
            match service.get_stack_with_context(number) {
                Ok(stack) => (stack.text.clone(), stack.text.len()),
                Err(e) => {
                    return Ok(IpcResp {
                        ok: false,
                        msg: e.to_string(),
                    });
                }
            }
        };

        // UI通知
        if let Ok(manager) = self.ui_manager.try_borrow() {
            let _ = manager.notify(UiNotification::StackAccessed(number));
        }

        // 直接入力実行
        match text_input::type_text(&stack_text).await {
            Ok(_) => Ok(IpcResp {
                ok: true,
                msg: UserFeedback::paste_success(number, char_count),
            }),
            Err(e) => Ok(IpcResp {
                ok: false,
                msg: format!("Failed to paste stack {}: {}", number, e),
            }),
        }
    }

    /// スタック一覧取得
    fn handle_list_stacks(&self) -> Result<IpcResp> {
        let service = self.stack.borrow();
        Ok(IpcResp {
            ok: true,
            msg: service.list_stacks_formatted(),
        })
    }

    /// スタッククリア
    fn handle_clear_stacks(&self) -> Result<IpcResp> {
        let mut service = self.stack.borrow_mut();
        let (_, message) = service.clear_stacks_with_confirmation();

        // UI通知
        if let Ok(manager) = self.ui_manager.try_borrow() {
            let _ = manager.notify(UiNotification::StacksCleared);
        }

        Ok(IpcResp {
            ok: true,
            msg: message,
        })
    }

    /// 自動停止タイマーをセットアップ
    fn setup_auto_stop_timer(&self) {
        let recording = self.recording.clone();
        let stack = self.stack.clone();
        let ui_manager = self.ui_manager.clone();
        let tx = self.transcription_tx.clone();
        let max_secs = recording.borrow().config().max_duration_secs;

        spawn_local(async move {
            // RecordingServiceからキャンセルレシーバーを取得
            let cancel_rx = recording.borrow().take_cancel_receiver();

            if let Some(cancel_rx) = cancel_rx {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(max_secs)) => {
                        // 30秒経過による自動停止
                        if recording.borrow().is_recording() {
                            println!("Auto-stop timer triggered after {}s", max_secs);
                            play_stop_sound();

                            if let Ok(result) = recording.borrow().stop_recording().await {
                                let (_, paste, direct_input, music_was_playing) =
                                    recording.borrow().get_context_info().unwrap_or((None, false, false, false));

                                let stack_for_transcription = if stack.borrow().is_stack_mode_enabled() {
                                    Some(stack.clone())
                                } else {
                                    None
                                };

                                let _ = tx.send((
                                    result,
                                    paste,
                                    music_was_playing,
                                    direct_input,
                                    stack_for_transcription,
                                    Some(ui_manager.clone()),
                                ));
                            }
                        }
                    }
                    _ = cancel_rx => {
                        // 手動停止によるキャンセル
                        println!("Auto-stop timer cancelled due to manual stop");
                    }
                }
            } else {
                println!("Warning: Could not set up auto-stop timer - no cancel receiver");
            }
        });
    }
}
