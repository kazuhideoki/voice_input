//! サービスコンテナ
//!
//! # 責任
//! - 全ての依存関係の構築と管理
//! - サービス間の依存関係の解決
//! - テスト時のモック注入サポート

use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;

use crate::application::{
    CommandHandler, MediaControlService, RecordingConfig, RecordingService, TranscriptionMessage,
    TranscriptionService, traits::TranscriptionClient,
};
use crate::domain::recorder::Recorder;
use crate::error::Result;
use crate::infrastructure::{
    audio::{AudioBackend, CpalAudioBackend},
    dict::JsonFileDictRepo,
    external::openai_adapter::OpenAiTranscriptionAdapter,
    ui::UiProcessManager,
};
use crate::shortcut::ShortcutService;

/// アプリケーション設定
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// 録音設定
    pub recording: RecordingConfig,
    /// 最大同時転写数
    pub max_concurrent_transcriptions: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            recording: RecordingConfig {
                max_duration_secs: std::env::var("VOICE_INPUT_MAX_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            },
            max_concurrent_transcriptions: 2,
        }
    }
}

/// サービスコンテナ
pub struct ServiceContainer<T: AudioBackend + 'static> {
    /// コマンドハンドラー
    pub command_handler: Rc<RefCell<CommandHandler<T>>>,
    /// ショートカットサービス（独立ワーカー用）
    pub shortcut_service: Rc<RefCell<ShortcutService>>,
    /// 転写メッセージ送信チャンネル
    pub transcription_tx: mpsc::UnboundedSender<TranscriptionMessage>,
    /// 転写メッセージ受信チャンネル
    pub transcription_rx: Option<mpsc::UnboundedReceiver<TranscriptionMessage>>,
}

impl ServiceContainer<CpalAudioBackend> {
    /// デフォルト設定で新しいServiceContainerを作成
    pub fn new() -> Result<Self> {
        let config = AppConfig::default();
        let recorder = Rc::new(RefCell::new(Recorder::new(CpalAudioBackend::default())));
        let client = Box::new(OpenAiTranscriptionAdapter::new()?);

        Self::with_dependencies(config, recorder, client)
    }

    /// テスト用の設定で作成
    #[cfg(test)]
    pub fn new_test() -> Result<Self> {
        use crate::application::service_container::test_helpers::MockTranscriptionClient;
        let config = AppConfig::default();
        let recorder = Rc::new(RefCell::new(Recorder::new(CpalAudioBackend::default())));
        let client = Box::new(MockTranscriptionClient::new("test transcription"));

        Self::with_dependencies(config, recorder, client)
    }
}

impl<T: AudioBackend + 'static> ServiceContainer<T> {
    /// カスタム設定で作成
    pub fn with_config(config: AppConfig) -> Result<Self>
    where
        T: Default,
    {
        let recorder = Rc::new(RefCell::new(Recorder::new(T::default())));
        let client = Box::new(OpenAiTranscriptionAdapter::new()?);

        Self::with_dependencies(config, recorder, client)
    }

    /// 依存関係を注入して作成（テスト用）
    pub fn with_dependencies(
        config: AppConfig,
        recorder: Rc<RefCell<Recorder<T>>>,
        transcription_client: Box<dyn TranscriptionClient>,
    ) -> Result<Self> {
        // 各サービスを構築
        let recording = Rc::new(RefCell::new(RecordingService::new(
            recorder,
            config.recording.clone(),
        )));

        let transcription = Rc::new(RefCell::new(TranscriptionService::new(
            transcription_client,
            Box::new(JsonFileDictRepo::new()),
            config.max_concurrent_transcriptions,
        )));

        let stack = Rc::new(RefCell::new(crate::application::StackService::new()));
        let media_control = Rc::new(RefCell::new(MediaControlService::new()));
        let ui_manager = Rc::new(RefCell::new(UiProcessManager::new()));
        let shortcut_service = Rc::new(RefCell::new(ShortcutService::new()));

        // 転写用チャンネル
        let (tx, rx) = mpsc::unbounded_channel();

        // コマンドハンドラーを構築
        let command_handler = Rc::new(RefCell::new(CommandHandler::new(
            recording,
            transcription,
            stack,
            media_control,
            ui_manager,
            shortcut_service.clone(),
            tx.clone(),
        )));

        Ok(ServiceContainer {
            command_handler,
            shortcut_service,
            transcription_tx: tx,
            transcription_rx: Some(rx),
        })
    }

    /// 転写受信チャンネルを取得（一度だけ）
    pub fn take_transcription_rx(
        &mut self,
    ) -> Option<mpsc::UnboundedReceiver<TranscriptionMessage>> {
        self.transcription_rx.take()
    }
}

/// テスト用のヘルパー実装
pub mod test_helpers {
    use super::*;
    use crate::infrastructure::audio::cpal_backend::AudioData;
    use async_trait::async_trait;

    /// テスト用のモックオーディオバックエンド
    pub struct MockAudioBackend {
        pub is_recording: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl Default for MockAudioBackend {
        fn default() -> Self {
            Self {
                is_recording: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }
    }

    impl AudioBackend for MockAudioBackend {
        fn start_recording(&self) -> std::result::Result<(), Box<dyn std::error::Error>> {
            self.is_recording
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        fn stop_recording(&self) -> std::result::Result<AudioData, Box<dyn std::error::Error>> {
            self.is_recording
                .store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(AudioData(vec![0u8; 100]))
        }

        fn is_recording(&self) -> bool {
            self.is_recording.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    /// テスト用のモック転写クライアント
    pub struct MockTranscriptionClient {
        pub response: String,
    }

    impl MockTranscriptionClient {
        pub fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
            }
        }
    }

    #[async_trait]
    impl TranscriptionClient for MockTranscriptionClient {
        async fn transcribe(&self, _audio: AudioData, _language: &str) -> Result<String> {
            Ok(self.response.clone())
        }
    }

    /// テスト用のServiceContainerビルダー
    pub struct TestServiceContainerBuilder {
        config: AppConfig,
        transcription_response: String,
    }

    impl TestServiceContainerBuilder {
        pub fn new() -> Self {
            Self {
                config: AppConfig::default(),
                transcription_response: "test transcription".to_string(),
            }
        }

        pub fn with_transcription_response(mut self, response: &str) -> Self {
            self.transcription_response = response.to_string();
            self
        }

        pub fn build(self) -> Result<ServiceContainer<MockAudioBackend>> {
            let recorder = Rc::new(RefCell::new(Recorder::new(MockAudioBackend::default())));
            let client = Box::new(MockTranscriptionClient::new(&self.transcription_response));

            ServiceContainer::with_dependencies(self.config, recorder, client)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;

    #[test]
    fn test_service_container_creation() {
        // テスト用のEnvConfig初期化
        crate::utils::config::EnvConfig::test_init();

        let container = TestServiceContainerBuilder::new()
            .build()
            .expect("Failed to create test container");

        assert!(container.transcription_rx.is_some());
    }

    #[test]
    fn test_take_transcription_rx() {
        // テスト用のEnvConfig初期化
        crate::utils::config::EnvConfig::test_init();

        let mut container = TestServiceContainerBuilder::new()
            .build()
            .expect("Failed to create test container");

        let rx = container.take_transcription_rx();
        assert!(rx.is_some());

        // 二回目はNone
        let rx2 = container.take_transcription_rx();
        assert!(rx2.is_none());
    }
}
