//! OpenAI STT API ラッパ。
//! WAV ファイルを multipart/form-data で転写エンドポイントに送信します。
use crate::infrastructure::audio::cpal_backend::AudioData;
use crate::utils::config::EnvConfig;
use crate::utils::profiling;
use reqwest::{Client, Proxy, multipart};
use serde::Deserialize;

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
        let config = EnvConfig::get();
        let api_key = config
            .openai_api_key
            .clone()
            .ok_or("OPENAI_API_KEY environment variable is not set")?;

        let model = std::env::var("OPENAI_TRANSCRIBE_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini-transcribe".to_string());

        let client =
            build_http_client().map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self {
            api_key,
            model,
            client,
        })
    }

    /// AudioDataから直接転写を実行
    pub async fn transcribe_audio(&self, audio_data: AudioData) -> Result<String, String> {
        if profiling::enabled() {
            profiling::log_point(
                "openai.request",
                &format!(
                    "bytes={} mime={} model={}",
                    audio_data.bytes.len(),
                    audio_data.mime_type,
                    self.model
                ),
            );
        }

        let part = multipart::Part::bytes(audio_data.bytes)
            .file_name(audio_data.file_name)
            .mime_str(audio_data.mime_type)
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
        let overall_timer = profiling::Timer::start("openai.transcribe_total");
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
        let request = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form);

        let send_timer = profiling::Timer::start("openai.send");
        let response = request
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;
        send_timer.log();

        let status = response.status();
        let read_timer = profiling::Timer::start("openai.read_body");
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        if profiling::enabled() {
            read_timer.log_with(&format!("status={} bytes={}", status, body.len()));
        } else {
            read_timer.log();
        }

        if !status.is_success() {
            if profiling::enabled() {
                overall_timer.log_with(&format!("status={}", status));
            } else {
                overall_timer.log();
            }
            return Err(format!(
                "API request failed with status {}: {}",
                status, body
            ));
        }

        let parse_timer = profiling::Timer::start("openai.parse_json");
        let transcription: TranscriptionResponse =
            serde_json::from_str(&body).map_err(|e| format!("Failed to parse response: {}", e))?;
        parse_timer.log();
        if profiling::enabled() {
            overall_timer.log_with(&format!(
                "status={} text_len={}",
                status,
                transcription.text.len()
            ));
        } else {
            overall_timer.log();
        }
        Ok(transcription.text)
    }
}

fn build_http_client() -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder().no_proxy();

    if let Some(proxy) = proxy_env("ALL_PROXY") {
        builder = builder.proxy(Proxy::all(&proxy)?);
    } else {
        if let Some(proxy) = proxy_env("HTTPS_PROXY") {
            builder = builder.proxy(Proxy::https(&proxy)?);
        }

        if let Some(proxy) = proxy_env("HTTP_PROXY") {
            builder = builder.proxy(Proxy::http(&proxy)?);
        }
    }

    builder.build()
}

fn proxy_env(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .or_else(|| {
            let lowercase = var.to_ascii_lowercase();
            std::env::var(&lowercase).ok()
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

// === Unit tests ==========================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::audio::cpal_backend::AudioData;

    /// 転写レスポンスのJSONをパースできる
    #[test]
    fn transcription_response_parses_json() {
        let json = r#"{"text":"こんにちは"}"#;
        let resp: TranscriptionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.text, "こんにちは");
    }

    /// APIキー有無に応じてクライアント生成結果が変わる
    #[tokio::test]
    async fn openai_client_new_respects_api_key_presence() {
        // テスト用の初期化（既に初期化済みなら何もしない）
        EnvConfig::test_init();

        // OpenAI APIキーが設定されているかどうかで挙動が変わる
        let client = OpenAiClient::new();

        // 環境変数またはテスト設定でAPIキーが設定されていれば成功
        // そうでなければ失敗
        if std::env::var("OPENAI_API_KEY").is_ok() || EnvConfig::get().openai_api_key.is_some() {
            assert!(client.is_ok());
        } else {
            assert!(client.is_err());
        }
    }

    /// ダミー音声での転写がエラーになることを確認する
    #[tokio::test]
    async fn transcribe_audio_rejects_dummy_memory_audio() {
        // テスト用の初期化
        EnvConfig::test_init();

        // OpenAI APIキーが設定されていない場合はテストをスキップ
        if EnvConfig::get().openai_api_key.is_none() && std::env::var("OPENAI_API_KEY").is_err() {
            println!("Skipping test: OPENAI_API_KEY not set");
            return;
        }

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

        let audio_data = AudioData {
            bytes: wav_data,
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };

        // This will fail with the actual API, but we're testing the method exists
        let result = client.transcribe_audio(audio_data).await;

        // We expect an error since we're using a test API key
        assert!(result.is_err());
    }

    /// 実在しないファイル相当の転写がエラーになることを確認する
    #[tokio::test]
    async fn transcribe_audio_rejects_missing_file_data() {
        // テスト用の初期化
        EnvConfig::test_init();

        // OpenAI APIキーが設定されていない場合はテストをスキップ
        if EnvConfig::get().openai_api_key.is_none() && std::env::var("OPENAI_API_KEY").is_err() {
            println!("Skipping test: OPENAI_API_KEY not set");
            return;
        }

        let client = OpenAiClient::new().unwrap();
        // メモリモードでのテスト
        let test_data = vec![1, 2, 3, 4];
        let audio_data = AudioData {
            bytes: test_data,
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };

        // This will fail because the file doesn't exist, but we're testing the method exists
        let result = client.transcribe_audio(audio_data).await;

        // We expect an error since the file doesn't exist
        assert!(result.is_err());
    }
}
