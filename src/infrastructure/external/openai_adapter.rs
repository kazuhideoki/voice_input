//! OpenAI クライアントのアダプター実装
//! Application層のTranscriptionClientトレイトを実装

use crate::application::AudioData;
use crate::application::{
    TranscriptionClient, TranscriptionClientError, TranscriptionClientOptions, TranscriptionEvent,
};
use crate::domain::transcription::TranscriptionOutput;
use crate::error::Result;
use crate::infrastructure::external::openai::OpenAiClient;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// OpenAI APIのアダプター
pub struct OpenAiTranscriptionAdapter {
    client: OpenAiClient,
}

impl OpenAiTranscriptionAdapter {
    /// 新しいアダプターを作成
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: OpenAiClient::new().map_err(|error| {
                crate::error::VoiceInputError::from(TranscriptionClientError::Initialization {
                    message: error.to_string(),
                })
            })?,
        })
    }

    /// 設定済みの OpenAI クライアントからアダプターを作成
    pub fn from_client(client: OpenAiClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TranscriptionClient for OpenAiTranscriptionAdapter {
    async fn transcribe(
        &self,
        audio: AudioData,
        options: &TranscriptionClientOptions,
    ) -> Result<TranscriptionOutput> {
        self.client
            .transcribe_audio_with_model(audio, options.model.as_deref())
            .await
            .map_err(|error| {
                crate::error::VoiceInputError::from(TranscriptionClientError::Request {
                    message: error.to_string(),
                })
            })
    }

    async fn transcribe_streaming(
        &self,
        audio: AudioData,
        options: &TranscriptionClientOptions,
        event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<TranscriptionOutput> {
        self.client
            .transcribe_audio_streaming_with_model(audio, options.model.as_deref(), event_tx)
            .await
            .map_err(|error| {
                crate::error::VoiceInputError::from(TranscriptionClientError::Request {
                    message: error.to_string(),
                })
            })
    }
}
