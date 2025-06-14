//! 音声録音を管理するサービス
//!
//! # 責任
//! - 録音の開始・停止
//! - 録音状態の管理
//! - 自動停止タイマーの管理

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use crate::domain::recorder::Recorder;
use crate::error::{Result, VoiceInputError};
use crate::infrastructure::audio::AudioBackend;
use crate::ipc::RecordingResult;

/// 録音状態
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingState {
    /// 待機中
    Idle,
    /// 録音中（セッションID付き）
    Recording(u64),
}

/// 録音設定
#[derive(Clone, Debug)]
pub struct RecordingConfig {
    /// 最大録音時間（秒）
    pub max_duration_secs: u64,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            max_duration_secs: 30,
        }
    }
}

/// 録音オプション
#[derive(Clone, Debug)]
pub struct RecordingOptions {
    /// 録音開始時のプロンプト
    pub prompt: Option<String>,
    /// ペーストフラグ
    pub paste: bool,
    /// 直接入力フラグ
    pub direct_input: bool,
}

/// 録音コンテキスト情報
#[derive(Debug)]
pub struct RecordingContext {
    /// 現在の状態
    pub state: RecordingState,
    /// 自動停止タイマーのキャンセル用
    pub cancel: Option<oneshot::Sender<()>>,
    /// 録音開始時にApple Musicが再生中だったか
    pub music_was_playing: bool,
    /// 録音開始時点で取得した選択テキストまたはCLIプロンプト
    pub start_prompt: Option<String>,
    /// 転写完了後にペーストを行うか
    pub paste: bool,
    /// 直接入力を使用するか
    pub direct_input: bool,
}

impl RecordingContext {
    pub fn new() -> Self {
        Self {
            state: RecordingState::Idle,
            cancel: None,
            music_was_playing: false,
            start_prompt: None,
            paste: false,
            direct_input: false,
        }
    }
}

impl Default for RecordingContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 録音サービス
pub struct RecordingService<T: AudioBackend> {
    /// レコーダー（既存の構造を維持）
    recorder: Rc<RefCell<Recorder<T>>>,
    /// 録音コンテキスト
    context: Arc<Mutex<RecordingContext>>,
    /// 設定
    config: RecordingConfig,
    /// セッションIDカウンター
    session_counter: Arc<Mutex<u64>>,
}

impl<T: AudioBackend> RecordingService<T> {
    /// 新しいRecordingServiceを作成
    pub fn new(recorder: Rc<RefCell<Recorder<T>>>, config: RecordingConfig) -> Self {
        Self {
            recorder,
            context: Arc::new(Mutex::new(RecordingContext::new())),
            config,
            session_counter: Arc::new(Mutex::new(0)),
        }
    }

    /// 録音コンテキストへの参照を取得
    pub fn context(&self) -> &Arc<Mutex<RecordingContext>> {
        &self.context
    }

    /// 録音を開始
    pub async fn start_recording(&self, options: RecordingOptions) -> Result<u64> {
        let mut ctx = self
            .context
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Context lock error: {}", e)))?;

        if ctx.state != RecordingState::Idle {
            return Err(VoiceInputError::RecordingAlreadyActive);
        }

        // セッションIDを生成
        let session_id = {
            let mut counter = self
                .session_counter
                .lock()
                .map_err(|e| VoiceInputError::SystemError(format!("Counter lock error: {}", e)))?;
            *counter += 1;
            *counter
        };

        // オプションを保存
        ctx.start_prompt = options.prompt;
        ctx.paste = options.paste;
        ctx.direct_input = options.direct_input;

        // レコーダーを開始
        self.recorder
            .borrow_mut()
            .start()
            .map_err(|e| VoiceInputError::AudioBackendError(e.to_string()))?;

        ctx.state = RecordingState::Recording(session_id);

        // 自動停止タイマーをセットアップ
        let (cancel_tx, _cancel_rx) = oneshot::channel::<()>();
        ctx.cancel = Some(cancel_tx);

        // タイマー処理は呼び出し元で実装（spawn_localの制約のため）

        Ok(session_id)
    }

    /// 録音を停止
    pub async fn stop_recording(&self) -> Result<RecordingResult> {
        let mut ctx = self
            .context
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Context lock error: {}", e)))?;

        match ctx.state {
            RecordingState::Idle => return Err(VoiceInputError::RecordingNotStarted),
            RecordingState::Recording(_) => {}
        }

        // 自動停止タイマーをキャンセル
        if let Some(cancel) = ctx.cancel.take() {
            let _ = cancel.send(());
        }

        // レコーダーを停止
        let audio_data = self
            .recorder
            .borrow_mut()
            .stop()
            .map_err(|e| VoiceInputError::AudioBackendError(e.to_string()))?;

        ctx.state = RecordingState::Idle;

        Ok(RecordingResult {
            audio_data: audio_data.into(),
            duration_ms: 0, // TODO: 実際の録音時間を計算
        })
    }

    /// 録音中かどうかを確認
    pub fn is_recording(&self) -> bool {
        if let Ok(ctx) = self.context.lock() {
            matches!(ctx.state, RecordingState::Recording(_))
        } else {
            false
        }
    }

    /// 自動停止キャンセルチャネルを取得（タイマー処理用）
    pub fn take_cancel_receiver(&self) -> Option<oneshot::Receiver<()>> {
        if let Ok(mut ctx) = self.context.lock() {
            if let Some(tx) = ctx.cancel.take() {
                let (new_tx, rx) = oneshot::channel();
                ctx.cancel = Some(new_tx);
                // 古いtxは破棄するだけで、send()は呼ばない
                // txが破棄されることで、対応するReceiverは自然にキャンセルされる
                drop(tx);
                Some(rx)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 設定を取得
    pub fn config(&self) -> &RecordingConfig {
        &self.config
    }

    /// 録音コンテキストの情報を取得
    pub fn get_context_info(&self) -> Result<(Option<String>, bool, bool, bool)> {
        let ctx = self
            .context
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Context lock error: {}", e)))?;
        Ok((
            ctx.start_prompt.clone(),
            ctx.paste,
            ctx.direct_input,
            ctx.music_was_playing,
        ))
    }

    /// Apple Music再生状態を設定
    pub fn set_music_was_playing(&self, was_playing: bool) -> Result<()> {
        let mut ctx = self
            .context
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Context lock error: {}", e)))?;
        ctx.music_was_playing = was_playing;
        Ok(())
    }
}
