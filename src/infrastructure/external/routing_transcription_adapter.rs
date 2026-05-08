//! 実行時オプションに応じて転写バックエンドを切り替えるアダプター

use once_cell::sync::OnceCell;
use tokio::sync::mpsc;

use crate::application::{
    AudioData, TranscriptionClient, TranscriptionClientError, TranscriptionClientOptions,
    TranscriptionEvent,
};
use crate::domain::transcription::TranscriptionOutput;
use crate::error::{Result, VoiceInputError};
use crate::infrastructure::external::{
    mlx_qwen3_asr_adapter::MlxQwen3AsrTranscriptionAdapter, openai::OpenAiClient,
    openai_adapter::OpenAiTranscriptionAdapter,
};
use crate::utils::config::{TranscriptionConfig, TranscriptionProvider};
use async_trait::async_trait;

/// リクエストごとに OpenAI と mlx-qwen3-asr を切り替える転写クライアント
pub struct RoutingTranscriptionAdapter {
    config: TranscriptionConfig,
    openai: OnceCell<OpenAiTranscriptionAdapter>,
    mlx_qwen3_asr: MlxQwen3AsrTranscriptionAdapter,
}

impl RoutingTranscriptionAdapter {
    /// 転写設定からルーティングアダプターを作成
    pub fn from_config(config: &TranscriptionConfig) -> Self {
        Self {
            config: config.clone(),
            openai: OnceCell::new(),
            mlx_qwen3_asr: MlxQwen3AsrTranscriptionAdapter::from_config(config),
        }
    }

    fn openai(&self) -> Result<&OpenAiTranscriptionAdapter> {
        self.openai.get_or_try_init(|| {
            OpenAiClient::from_config(&self.config)
                .map(OpenAiTranscriptionAdapter::from_client)
                .map_err(|error| {
                    VoiceInputError::from(TranscriptionClientError::Initialization {
                        message: error.to_string(),
                    })
                })
        })
    }
}

#[async_trait]
impl TranscriptionClient for RoutingTranscriptionAdapter {
    async fn transcribe(
        &self,
        audio: AudioData,
        options: &TranscriptionClientOptions,
    ) -> Result<TranscriptionOutput> {
        match options.provider {
            TranscriptionProvider::OpenAi => self.openai()?.transcribe(audio, options).await,
            TranscriptionProvider::MlxQwen3Asr => {
                self.mlx_qwen3_asr.transcribe(audio, options).await
            }
        }
    }

    async fn transcribe_streaming(
        &self,
        audio: AudioData,
        options: &TranscriptionClientOptions,
        event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<TranscriptionOutput> {
        match options.provider {
            TranscriptionProvider::OpenAi => {
                self.openai()?
                    .transcribe_streaming(audio, options, event_tx)
                    .await
            }
            TranscriptionProvider::MlxQwen3Asr => {
                self.mlx_qwen3_asr.transcribe(audio, options).await
            }
        }
    }
}
