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
    CommandHandler, MediaControlService, RecordingConfig, RecordingService, TranscriptionClient,
    TranscriptionMessage, TranscriptionService,
};
use crate::domain::recorder::Recorder;
use crate::error::Result;
use crate::infrastructure::{
    audio::{AudioBackend, CpalAudioBackend},
    dict::JsonFileDictRepo,
    external::openai_adapter::OpenAiTranscriptionAdapter,
};

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

        let media_control = Rc::new(RefCell::new(MediaControlService::new()));

        // 転写用チャンネル
        let (tx, rx) = mpsc::unbounded_channel();

        // コマンドハンドラーを構築
        let command_handler = Rc::new(RefCell::new(CommandHandler::new(
            recording,
            transcription,
            media_control,
            tx.clone(),
        )));

        Ok(ServiceContainer {
            command_handler,
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
#[cfg(test)]
pub mod test_helpers {
    use super::*;
    use crate::application::{
        CommandHandler, MediaControlService, RecordingConfig, RecordingService,
        TranscriptionService,
    };
    use crate::infrastructure::audio::cpal_backend::AudioData;
    use async_trait::async_trait;
    use tokio::sync::mpsc;

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
            Ok(AudioData {
                bytes: vec![0u8; 100],
                mime_type: "audio/wav",
                file_name: "audio.wav".to_string(),
            })
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
        transcription_response: String,
        auto_stop_duration: Option<u64>,
    }

    impl Default for TestServiceContainerBuilder {
        fn default() -> Self {
            Self::new()
        }
    }

    impl TestServiceContainerBuilder {
        pub fn new() -> Self {
            Self {
                transcription_response: "test transcription".to_string(),
                auto_stop_duration: None,
            }
        }

        pub fn with_transcription_response(mut self, response: &str) -> Self {
            self.transcription_response = response.to_string();
            self
        }

        pub fn with_auto_stop_duration(mut self, duration_secs: u64) -> Self {
            self.auto_stop_duration = Some(duration_secs);
            self
        }

        pub async fn build(self) -> Result<ServiceContainer<MockAudioBackend>> {
            // EnvConfig初期化
            let _ = crate::utils::config::EnvConfig::init();

            let recorder = Rc::new(RefCell::new(Recorder::new(MockAudioBackend::default())));
            let client = Box::new(MockTranscriptionClient::new(&self.transcription_response));

            // 録音設定を構築
            let mut recording_config = RecordingConfig::default();
            if let Some(duration) = self.auto_stop_duration {
                recording_config.max_duration_secs = duration;
            }

            // RecordingServiceを作成
            let recording_service = Rc::new(RefCell::new(RecordingService::new(
                recorder.clone(),
                recording_config,
            )));

            // 他のサービスを作成
            let transcription_service = Rc::new(RefCell::new(
                TranscriptionService::with_default_repo(client),
            ));
            let media_control_service = Rc::new(RefCell::new(MediaControlService::new()));

            // 転写ワーカー用のチャンネル
            let (transcription_tx, transcription_rx) = mpsc::unbounded_channel();

            // CommandHandlerを作成
            let command_handler = Rc::new(RefCell::new(CommandHandler::new(
                recording_service,
                transcription_service,
                media_control_service,
                transcription_tx.clone(),
            )));

            Ok(ServiceContainer {
                command_handler,
                transcription_tx,
                transcription_rx: Some(transcription_rx),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;

    /// テスト用のサービスコンテナを構築できる
    #[tokio::test]
    async fn service_container_can_be_built() {
        // テスト用のEnvConfig初期化
        let _ = crate::utils::config::EnvConfig::init();

        let container = TestServiceContainerBuilder::new()
            .build()
            .await
            .expect("Failed to create test container");

        assert!(container.transcription_rx.is_some());
    }

    /// transcription_rxは一度だけ取得できる
    #[tokio::test]
    async fn transcription_rx_can_be_taken_once() {
        // テスト用のEnvConfig初期化
        let _ = crate::utils::config::EnvConfig::init();

        let mut container = TestServiceContainerBuilder::new()
            .build()
            .await
            .expect("Failed to create test container");

        let rx = container.take_transcription_rx();
        assert!(rx.is_some());

        // 二回目はNone
        let rx2 = container.take_transcription_rx();
        assert!(rx2.is_none());
    }
}
