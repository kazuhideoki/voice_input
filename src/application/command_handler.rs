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
    MediaControlService, RecordingOptions, RecordingService, TranscriptionService,
};
use crate::error::{Result, VoiceInputError};
use crate::infrastructure::{
    audio::{AudioBackend, CpalAudioBackend},
    external::sound::{play_start_sound, play_stop_sound},
};
use crate::ipc::{IpcCmd, IpcResp, RecordingResult};

/// 転写メッセージ
pub type TranscriptionMessage = (
    RecordingResult,
    bool, // paste
    bool, // resume_music
    bool, // direct_input
);

/// コマンドハンドラー
pub struct CommandHandler<T: AudioBackend> {
    recording: Rc<RefCell<RecordingService<T>>>,
    #[allow(dead_code)]
    transcription: Rc<RefCell<TranscriptionService>>,
    media_control: Rc<RefCell<MediaControlService>>,
    transcription_tx: mpsc::UnboundedSender<TranscriptionMessage>,
}

impl<T: AudioBackend + 'static> CommandHandler<T> {
    /// 新しいCommandHandlerを作成
    pub fn new(
        recording: Rc<RefCell<RecordingService<T>>>,
        transcription: Rc<RefCell<TranscriptionService>>,
        media_control: Rc<RefCell<MediaControlService>>,
        transcription_tx: mpsc::UnboundedSender<TranscriptionMessage>,
    ) -> Self {
        Self {
            recording,
            transcription,
            media_control,
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

        // 転写キューに送信
        self.transcription_tx
            .send((result, paste, music_was_playing, direct_input))
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

    /// 自動停止タイマーをセットアップ
    fn setup_auto_stop_timer(&self) {
        let recording = self.recording.clone();
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
                                let _ = tx.send((
                                    result,
                                    paste,
                                    music_was_playing,
                                    direct_input,
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
