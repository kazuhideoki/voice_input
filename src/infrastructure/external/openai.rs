//! OpenAI STT API ラッパ。
//! WAV ファイルを multipart/form-data で転写エンドポイントに送信します。
use crate::infrastructure::audio::cpal_backend::AudioData;
use reqwest::multipart;
use serde::Deserialize;
use std::env;

/// STT API のレスポンス JSON。
#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    pub text: String,
}

/// Dictionary suggestion (surface -> replacement)
#[derive(Debug, Deserialize)]
pub struct WordSuggestion {
    pub surface: String,
    pub replacement: String,
}

/// OpenAI API client
pub struct OpenAiClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiClient {
    /// Create a new OpenAI client
    pub fn new() -> Result<Self, String> {
        let api_key = env::var("OPENAI_API_KEY")
            .map_err(|_| "OPENAI_API_KEY environment variable not set")?;

        let model = env::var("OPENAI_TRANSCRIBE_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini-transcribe".to_string());

        Ok(Self {
            api_key,
            model,
            client: reqwest::Client::new(),
        })
    }

    /// AudioDataから直接転写を実行
    pub async fn transcribe_audio(&self, audio_data: AudioData) -> Result<String, String> {
        let wav_data = match audio_data {
            AudioData::Memory(data) => data,
            AudioData::File(path) => {
                // 後方互換性: ファイルから読み込み
                std::fs::read(&path).map_err(|e| format!("Failed to read audio file: {}", e))?
            }
        };

        let part = multipart::Part::bytes(wav_data)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| format!("Failed to create multipart: {}", e))?;

        // 既存の転写処理を実行
        self.transcribe_with_part(part, None).await
    }


    /// 共通の転写処理
    async fn transcribe_with_part(
        &self,
        file_part: multipart::Part,
        prompt: Option<&str>,
    ) -> Result<String, String> {
        let url = "https://api.openai.com/v1/audio/transcriptions";

        // multipart/form-data
        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("language", "ja");

        if let Some(prompt_text) = prompt {
            let formatted_prompt = format!(
                "The following text provides relevant context. Please consider this when creating the transcription: {:?}",
                prompt_text
            );
            form = form.text("prompt", formatted_prompt);
        }

        // 送信
        let response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if !status.is_success() {
            return Err(format!(
                "API request failed with status {}: {}",
                status, body
            ));
        }

        let transcription: TranscriptionResponse =
            serde_json::from_str(&body).map_err(|e| format!("Failed to parse response: {}", e))?;
        Ok(transcription.text)
    }
}

// === Unit tests ==========================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::audio::cpal_backend::AudioData;
    use std::path::PathBuf;

    #[test]
    fn parse_transcription_response_json() {
        let json = r#"{"text":"こんにちは"}"#;
        let resp: TranscriptionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.text, "こんにちは");
    }

    #[tokio::test]
    async fn test_openai_client_new() {
        // Test with API key set
        unsafe { env::set_var("OPENAI_API_KEY", "test-key") };
        let client = OpenAiClient::new();
        assert!(client.is_ok());

        // Test without API key
        unsafe { env::remove_var("OPENAI_API_KEY") };
        let client = OpenAiClient::new();
        assert!(client.is_err());
    }

    #[tokio::test]
    async fn test_transcribe_audio_memory() {
        unsafe { env::set_var("OPENAI_API_KEY", "test-key") };

        let client = OpenAiClient::new().unwrap();

        // Create a minimal WAV header for testing
        let wav_data = vec![
            0x52, 0x49, 0x46, 0x46, // "RIFF"
            0x24, 0x00, 0x00, 0x00, // file size - 8
            0x57, 0x41, 0x56, 0x45, // "WAVE"
            0x66, 0x6d, 0x74, 0x20, // "fmt "
            0x10, 0x00, 0x00, 0x00, // fmt chunk size
            0x01, 0x00, // PCM format
            0x01, 0x00, // 1 channel
            0x22, 0x56, 0x00, 0x00, // 22050 sample rate
            0x44, 0xac, 0x00, 0x00, // byte rate
            0x02, 0x00, // block align
            0x10, 0x00, // bits per sample
            0x64, 0x61, 0x74, 0x61, // "data"
            0x00, 0x00, 0x00, 0x00, // data size
        ];

        let audio_data = AudioData::Memory(wav_data);

        // This will fail with the actual API, but we're testing the method exists
        let result = client.transcribe_audio(audio_data).await;

        // We expect an error since we're using a test API key
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transcribe_audio_file() {
        unsafe { env::set_var("OPENAI_API_KEY", "test-key") };

        let client = OpenAiClient::new().unwrap();
        let audio_data = AudioData::File(PathBuf::from("/tmp/test.wav"));

        // This will fail because the file doesn't exist, but we're testing the method exists
        let result = client.transcribe_audio(audio_data).await;

        // We expect an error since the file doesn't exist
        assert!(result.is_err());
    }

}
