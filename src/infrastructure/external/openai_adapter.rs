//! OpenAI クライアントのアダプター実装
//! Application層のTranscriptionClientトレイトを実装

use crate::application::traits::TranscriptionClient;
use crate::error::Result;
use crate::infrastructure::audio::cpal_backend::AudioData;
use crate::infrastructure::external::openai::OpenAiClient;
use async_trait::async_trait;

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
    async fn transcribe(&self, audio: AudioData, _language: &str) -> Result<String> {
        self.client
            .transcribe_audio(audio)
            .await
            .map_err(|e| crate::error::VoiceInputError::TranscriptionFailed(e))
    }
}
