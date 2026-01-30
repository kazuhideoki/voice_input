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
}

impl RecordingContext {
    pub fn new() -> Self {
        Self {
            state: RecordingState::Idle,
            cancel: None,
            music_was_playing: false,
            start_prompt: None,
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
    pub config: RecordingConfig,
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
    pub fn get_context_info(&self) -> Result<(Option<String>, bool)> {
        let ctx = self
            .context
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Context lock error: {}", e)))?;
        Ok((ctx.start_prompt.clone(), ctx.music_was_playing))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::recorder::Recorder;
    use crate::infrastructure::audio::cpal_backend::AudioData;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tokio::time::timeout;

    /// テスト用のモックオーディオバックエンド
    struct MockAudioBackend {
        is_recording: Arc<AtomicBool>,
    }

    impl MockAudioBackend {
        fn new() -> Self {
            Self {
                is_recording: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl crate::infrastructure::audio::AudioBackend for MockAudioBackend {
        fn start_recording(&self) -> std::result::Result<(), Box<dyn std::error::Error>> {
            self.is_recording.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn stop_recording(&self) -> std::result::Result<AudioData, Box<dyn std::error::Error>> {
            self.is_recording.store(false, Ordering::SeqCst);
            Ok(AudioData {
                bytes: vec![0u8; 100],
                mime_type: "audio/wav",
                file_name: "audio.wav".to_string(),
            })
        }

        fn is_recording(&self) -> bool {
            self.is_recording.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn test_cancel_channel_behavior() {
        // RecordingServiceを作成
        let backend = MockAudioBackend::new();
        let recorder = Rc::new(RefCell::new(Recorder::new(backend)));
        let config = RecordingConfig {
            max_duration_secs: 30,
        };
        let service = RecordingService::new(recorder, config);

        // 録音開始
        let options = RecordingOptions { prompt: None };
        service.start_recording(options).await.unwrap();

        // キャンセルレシーバーを取得
        let cancel_rx = service.take_cancel_receiver();
        assert!(cancel_rx.is_some(), "Should get cancel receiver");

        // キャンセルレシーバーが即座に発火しないことを確認
        let cancel_rx = cancel_rx.unwrap();
        let result = timeout(Duration::from_millis(100), cancel_rx).await;
        assert!(
            result.is_err(),
            "Cancel receiver should not fire immediately"
        );

        // 録音停止
        service.stop_recording().await.unwrap();

        // 新しいキャンセルレシーバーを取得できないことを確認（既に停止済み）
        let cancel_rx2 = service.take_cancel_receiver();
        assert!(
            cancel_rx2.is_none(),
            "Should not get cancel receiver after stop"
        );
    }

    #[tokio::test]
    async fn test_multiple_start_stop_cycles() {
        let backend = MockAudioBackend::new();
        let recorder = Rc::new(RefCell::new(Recorder::new(backend)));
        let config = RecordingConfig {
            max_duration_secs: 30,
        };
        let service = RecordingService::new(recorder, config);

        // 3回の開始・停止サイクルを実行
        for i in 0..3 {
            // 録音開始
            let options = RecordingOptions {
                prompt: Some(format!("Test {}", i)),
            };
            let session_id = service.start_recording(options).await.unwrap();
            assert!(session_id > 0, "Session ID should be positive");
            assert!(service.is_recording(), "Should be recording after start");

            // キャンセルレシーバーを取得
            let cancel_rx = service.take_cancel_receiver();
            assert!(
                cancel_rx.is_some(),
                "Should get cancel receiver for cycle {}",
                i
            );

            // 少し待機
            tokio::time::sleep(Duration::from_millis(50)).await;

            // 録音停止
            let result = service.stop_recording().await.unwrap();
            assert!(!result.audio_data.0.is_empty(), "Should have audio data");
            assert!(
                !service.is_recording(),
                "Should not be recording after stop"
            );
        }
    }

    #[test]
    fn test_context_state_transitions() {
        let backend = MockAudioBackend::new();
        let recorder = Rc::new(RefCell::new(Recorder::new(backend)));
        let config = RecordingConfig {
            max_duration_secs: 30,
        };
        let service = RecordingService::new(recorder, config);

        // 初期状態の確認
        {
            let ctx = service.context.lock().unwrap();
            assert_eq!(ctx.state, RecordingState::Idle);
            assert!(ctx.cancel.is_none());
            assert!(!ctx.music_was_playing);
        }

        // 音楽再生状態を設定
        service.set_music_was_playing(true).unwrap();
        {
            let ctx = service.context.lock().unwrap();
            assert!(ctx.music_was_playing);
        }

        // コンテキスト情報を取得
        let (prompt, music_was_playing) = service.get_context_info().unwrap();
        assert!(prompt.is_none());
        assert!(music_was_playing);
    }
}
