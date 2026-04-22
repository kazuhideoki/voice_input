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

use crate::application::{AudioBackend, AudioData, Recorder};
use crate::error::{Result, VoiceInputError};

/// 録音状態
#[derive(Debug)]
pub enum RecordingState {
    /// 待機中
    Idle,
    /// 録音中
    Recording(ActiveRecordingSession),
}

/// 録音中セッション
#[derive(Debug)]
pub struct ActiveRecordingSession {
    /// セッションID
    pub session_id: u64,
    /// 自動停止タイマーのキャンセル用
    pub cancel: Option<oneshot::Sender<()>>,
    /// 録音開始時にApple Musicが再生中だったか
    pub music_was_playing: bool,
    /// 録音開始時点で取得した選択テキストまたはCLIプロンプト
    pub start_prompt: Option<String>,
}

impl ActiveRecordingSession {
    fn new(session_id: u64, options: RecordingOptions) -> Self {
        let (cancel, _cancel_rx) = oneshot::channel::<()>();
        Self {
            session_id,
            cancel: Some(cancel),
            music_was_playing: false,
            start_prompt: options.prompt,
        }
    }
}

impl PartialEq for RecordingState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Idle, Self::Idle) => true,
            (Self::Recording(lhs), Self::Recording(rhs)) => lhs.session_id == rhs.session_id,
            _ => false,
        }
    }
}

impl Eq for RecordingState {}

impl RecordingState {
    fn is_recording(&self) -> bool {
        matches!(self, Self::Recording(_))
    }

    fn active_session_id(&self) -> Option<u64> {
        match self {
            Self::Idle => None,
            Self::Recording(session) => Some(session.session_id),
        }
    }

    fn context_info(&self) -> (Option<String>, bool) {
        match self {
            Self::Idle => (None, false),
            Self::Recording(session) => (session.start_prompt.clone(), session.music_was_playing),
        }
    }

    fn set_music_was_playing(&mut self, was_playing: bool) {
        if let Self::Recording(session) = self {
            session.music_was_playing = was_playing;
        }
    }

    fn take_cancel_receiver(&mut self) -> Option<oneshot::Receiver<()>> {
        match self {
            Self::Idle => None,
            Self::Recording(session) => {
                let tx = session.cancel.take()?;
                let (new_tx, rx) = oneshot::channel();
                session.cancel = Some(new_tx);
                drop(tx);
                Some(rx)
            }
        }
    }

    fn stopped_context(&self) -> Result<StoppedSessionContext> {
        match self {
            Self::Idle => Err(VoiceInputError::RecordingNotStarted),
            Self::Recording(session) => Ok(StoppedSessionContext {
                session_id: session.session_id,
                start_prompt: session.start_prompt.clone(),
                music_was_playing: session.music_was_playing,
            }),
        }
    }
}

/// 停止済み録音セッションの文脈
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoppedSessionContext {
    pub session_id: u64,
    pub start_prompt: Option<String>,
    pub music_was_playing: bool,
}

/// 録音停止結果
#[derive(Clone, Debug)]
pub struct RecordedAudio {
    pub audio_data: AudioData,
    pub duration_ms: u64,
}

/// 録音停止結果
#[derive(Clone, Debug)]
pub struct StopRecordingOutcome {
    pub result: RecordedAudio,
    pub context: StoppedSessionContext,
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
}

impl RecordingContext {
    pub fn new() -> Self {
        Self {
            state: RecordingState::Idle,
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

        // レコーダーを開始
        self.recorder
            .borrow_mut()
            .start()
            .map_err(VoiceInputError::from)?;

        ctx.state = RecordingState::Recording(ActiveRecordingSession::new(session_id, options));

        // タイマー処理は呼び出し元で実装（spawn_localの制約のため）

        Ok(session_id)
    }

    /// 録音を停止
    pub async fn stop_recording(&self) -> Result<StopRecordingOutcome> {
        let mut ctx = self
            .context
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Context lock error: {}", e)))?;

        let stopped_context = ctx.state.stopped_context()?;
        if let RecordingState::Recording(session) = &mut ctx.state {
            if let Some(cancel) = session.cancel.take() {
                let _ = cancel.send(());
            }
        }

        // レコーダーを停止
        let audio_data = self
            .recorder
            .borrow_mut()
            .stop()
            .map_err(VoiceInputError::from)?;

        ctx.state = RecordingState::Idle;

        Ok(StopRecordingOutcome {
            result: RecordedAudio {
                audio_data,
                duration_ms: 0, // TODO: 実際の録音時間を計算
            },
            context: stopped_context,
        })
    }

    /// 録音中かどうかを確認
    pub fn is_recording(&self) -> bool {
        if let Ok(ctx) = self.context.lock() {
            ctx.state.is_recording()
        } else {
            false
        }
    }

    /// 指定したセッションが現在も録音中かを確認
    pub fn is_active_session(&self, session_id: u64) -> Result<bool> {
        let ctx = self
            .context
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Context lock error: {}", e)))?;
        Ok(ctx.state.active_session_id() == Some(session_id))
    }

    /// 指定したセッションより新しい録音開始が発生したかを確認
    pub fn has_started_newer_session(&self, session_id: u64) -> Result<bool> {
        let counter = self
            .session_counter
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Counter lock error: {}", e)))?;
        Ok(*counter > session_id)
    }

    /// 自動停止キャンセルチャネルを取得（タイマー処理用）
    pub fn take_cancel_receiver(&self) -> Option<oneshot::Receiver<()>> {
        if let Ok(mut ctx) = self.context.lock() {
            ctx.state.take_cancel_receiver()
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
        Ok(ctx.state.context_info())
    }

    /// Apple Music再生状態を設定
    pub fn set_music_was_playing(&self, was_playing: bool) -> Result<()> {
        let mut ctx = self
            .context
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Context lock error: {}", e)))?;
        ctx.state.set_music_was_playing(was_playing);
        Ok(())
    }

    /// スリープ復帰後に録音系リソースを回復する
    pub fn recover_after_wake(&self) -> Result<()> {
        if self.is_recording() {
            return Ok(());
        }

        self.recorder
            .borrow()
            .recover_after_wake()
            .map_err(VoiceInputError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{AudioData, Recorder};
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

    struct FailingStopAudioBackend {
        is_recording: Arc<AtomicBool>,
    }

    struct RecoverableAudioBackend {
        is_recording: Arc<AtomicBool>,
        recover_calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FailingStopAudioBackend {
        fn new() -> Self {
            Self {
                is_recording: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl RecoverableAudioBackend {
        fn new() -> Self {
            Self {
                is_recording: Arc::new(AtomicBool::new(false)),
                recover_calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            }
        }
    }

    impl crate::application::AudioBackend for MockAudioBackend {
        fn start_recording(
            &self,
        ) -> std::result::Result<(), crate::application::AudioBackendError> {
            self.is_recording.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn stop_recording(
            &self,
        ) -> std::result::Result<AudioData, crate::application::AudioBackendError> {
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

    impl crate::application::AudioBackend for FailingStopAudioBackend {
        fn start_recording(
            &self,
        ) -> std::result::Result<(), crate::application::AudioBackendError> {
            self.is_recording.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn stop_recording(
            &self,
        ) -> std::result::Result<AudioData, crate::application::AudioBackendError> {
            Err(crate::application::AudioBackendError::StreamOperation {
                message: "stop failed".to_string(),
            })
        }

        fn is_recording(&self) -> bool {
            self.is_recording.load(Ordering::SeqCst)
        }
    }

    impl crate::application::AudioBackend for RecoverableAudioBackend {
        fn start_recording(
            &self,
        ) -> std::result::Result<(), crate::application::AudioBackendError> {
            self.is_recording.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn stop_recording(
            &self,
        ) -> std::result::Result<AudioData, crate::application::AudioBackendError> {
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

        fn recover_after_wake(
            &self,
        ) -> std::result::Result<(), crate::application::AudioBackendError> {
            self.recover_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// 録音中のみキャンセルチャネルが取得できる
    #[tokio::test]
    async fn cancel_channel_available_only_while_recording() {
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

    /// 複数回の開始・停止サイクルが成立する
    #[tokio::test]
    async fn multiple_start_stop_cycles_work() {
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
            assert!(
                !result.result.audio_data.bytes.is_empty(),
                "Should have audio data"
            );
            assert!(
                !service.is_recording(),
                "Should not be recording after stop"
            );
        }
    }

    /// 待機中に復帰回復を呼ぶとバックエンドに委譲する
    #[test]
    fn recover_after_wake_delegates_to_backend_while_idle() {
        let backend = RecoverableAudioBackend::new();
        let recover_calls = backend.recover_calls.clone();
        let recorder = Rc::new(RefCell::new(Recorder::new(backend)));
        let config = RecordingConfig {
            max_duration_secs: 30,
        };
        let service = RecordingService::new(recorder, config);

        service.recover_after_wake().unwrap();

        assert_eq!(recover_calls.load(Ordering::SeqCst), 1);
    }

    /// 録音中は復帰回復をスキップする
    #[tokio::test]
    async fn recover_after_wake_skips_backend_while_recording() {
        let backend = RecoverableAudioBackend::new();
        let recover_calls = backend.recover_calls.clone();
        let recorder = Rc::new(RefCell::new(Recorder::new(backend)));
        let config = RecordingConfig {
            max_duration_secs: 30,
        };
        let service = RecordingService::new(recorder, config);

        service
            .start_recording(RecordingOptions { prompt: None })
            .await
            .unwrap();

        service.recover_after_wake().unwrap();

        assert_eq!(recover_calls.load(Ordering::SeqCst), 0);
    }

    /// コンテキスト状態が期待通りに遷移する
    #[test]
    fn context_state_transitions_are_consistent() {
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
            assert_eq!(ctx.state.context_info(), (None, false));
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            service
                .start_recording(RecordingOptions {
                    prompt: Some("prompt".to_string()),
                })
                .await
                .unwrap();
        });

        service.set_music_was_playing(true).unwrap();
        let (prompt, music_was_playing) = service.get_context_info().unwrap();
        assert_eq!(prompt, Some("prompt".to_string()));
        assert!(music_was_playing);
    }

    /// 現在のセッション一致判定が取得できる
    #[tokio::test]
    async fn active_session_matches_only_current_recording() {
        let backend = MockAudioBackend::new();
        let recorder = Rc::new(RefCell::new(Recorder::new(backend)));
        let config = RecordingConfig {
            max_duration_secs: 30,
        };
        let service = RecordingService::new(recorder, config);

        let first_session = service
            .start_recording(RecordingOptions { prompt: None })
            .await
            .unwrap();
        assert!(service.is_active_session(first_session).unwrap());
        assert!(!service.is_active_session(first_session + 1).unwrap());

        service.stop_recording().await.unwrap();
        assert!(!service.is_active_session(first_session).unwrap());

        let second_session = service
            .start_recording(RecordingOptions { prompt: None })
            .await
            .unwrap();
        assert_ne!(first_session, second_session);
        assert!(service.is_active_session(second_session).unwrap());
        assert!(!service.is_active_session(first_session).unwrap());
    }

    /// 新しい録音開始が発生した事実を保持できる
    #[tokio::test]
    async fn newer_session_start_is_detected_after_restart() {
        let backend = MockAudioBackend::new();
        let recorder = Rc::new(RefCell::new(Recorder::new(backend)));
        let config = RecordingConfig {
            max_duration_secs: 30,
        };
        let service = RecordingService::new(recorder, config);

        let first_session = service
            .start_recording(RecordingOptions { prompt: None })
            .await
            .unwrap();
        assert!(!service.has_started_newer_session(first_session).unwrap());

        service.stop_recording().await.unwrap();
        assert!(!service.has_started_newer_session(first_session).unwrap());

        let second_session = service
            .start_recording(RecordingOptions { prompt: None })
            .await
            .unwrap();

        assert_ne!(first_session, second_session);
        assert!(service.has_started_newer_session(first_session).unwrap());
        assert!(!service.has_started_newer_session(second_session).unwrap());
    }

    /// 録音停止失敗時も録音状態と文脈が維持される
    #[tokio::test]
    async fn stop_failure_keeps_active_session_state() {
        let backend = FailingStopAudioBackend::new();
        let recorder = Rc::new(RefCell::new(Recorder::new(backend)));
        let config = RecordingConfig {
            max_duration_secs: 30,
        };
        let service = RecordingService::new(recorder, config);

        let session_id = service
            .start_recording(RecordingOptions {
                prompt: Some("prompt".to_string()),
            })
            .await
            .unwrap();
        service.set_music_was_playing(true).unwrap();

        let error = service.stop_recording().await.unwrap_err();

        assert!(matches!(error, VoiceInputError::AudioBackendError(_)));
        assert!(
            std::error::Error::source(&error).is_some(),
            "audio backend failure should preserve the typed source error"
        );
        assert!(service.is_recording());
        assert!(service.is_active_session(session_id).unwrap());
        assert_eq!(
            service.get_context_info().unwrap(),
            (Some("prompt".to_string()), true)
        );
    }
}
