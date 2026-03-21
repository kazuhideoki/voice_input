//! OpenAI クライアントのアダプター実装
//! Application層のTranscriptionClientトレイトを実装

use crate::application::{TranscriptionClient, TranscriptionEvent, TranscriptionOutput};
use crate::domain::audio::AudioData;
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
            client: OpenAiClient::new()?,
        })
    }
}

#[async_trait]
impl TranscriptionClient for OpenAiTranscriptionAdapter {
    async fn transcribe(&self, audio: AudioData, _language: &str) -> Result<TranscriptionOutput> {
        self.client
            .transcribe_audio(audio)
            .await
            .map_err(crate::error::VoiceInputError::TranscriptionFailed)
    }

    async fn transcribe_streaming(
        &self,
        audio: AudioData,
        _language: &str,
        event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<TranscriptionOutput> {
        self.client
            .transcribe_audio_streaming(audio, event_tx)
            .await
            .map_err(crate::error::VoiceInputError::TranscriptionFailed)
    }
}
