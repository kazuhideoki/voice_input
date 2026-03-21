//! OpenAI STT API ラッパ。
//! AudioData（既定: FLAC、失敗時にWAVへフォールバック）を
//! multipart/form-data で転写エンドポイントに送信します。
use crate::application::{TranscriptionEvent, TranscriptionOutput, TranscriptionToken};
use crate::infrastructure::audio::cpal_backend::AudioData;
use crate::utils::config::EnvConfig;
use crate::utils::profiling;
use reqwest::{Client, Proxy, multipart};
use serde::Deserialize;
use tokio::sync::mpsc;

/// STT API のレスポンス JSON。
#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    pub text: String,
    #[serde(default)]
    pub logprobs: Vec<TokenLogprobResponse>,
}

#[derive(Debug, Deserialize)]
struct StreamingDeltaResponse {
    pub delta: String,
}

#[derive(Debug, Deserialize)]
struct StreamingCompletedResponse {
    pub text: String,
    #[serde(default)]
    pub logprobs: Vec<TokenLogprobResponse>,
}

#[derive(Debug, Deserialize)]
struct StreamingEventEnvelope {
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    pub delta: Option<String>,
    pub text: Option<String>,
    pub logprobs: Option<Vec<TokenLogprobResponse>>,
}

#[derive(Clone, Debug, Deserialize)]
struct TokenLogprobResponse {
    pub token: String,
    pub logprob: f64,
}

#[derive(Clone, Debug, PartialEq)]
enum StreamingTranscriptionEvent {
    Delta(String),
    Completed(TranscriptionOutput),
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
            .transcription
            .openai_api_key
            .clone()
            .ok_or("OPENAI_API_KEY environment variable is not set")?;

<<<<<<< HEAD
        let model = config.transcription.model.as_str().to_string();
        if config.transcription.streaming_enabled
            && !config.transcription.model.supports_streaming()
        {
            return Err(format!(
                "OPENAI_TRANSCRIBE_MODEL={} does not support streaming",
                model
            ));
        }
=======
        let model = config.openai_transcribe_model.as_str().to_string();
>>>>>>> main

        let client =
            build_http_client().map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self {
            api_key,
            model,
            client,
        })
    }

    /// AudioDataから直接転写を実行
    pub async fn transcribe_audio(
        &self,
        audio_data: AudioData,
    ) -> Result<TranscriptionOutput, String> {
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

    /// AudioDataから直接ストリーミング転写を実行
    pub async fn transcribe_audio_streaming(
        &self,
        audio_data: AudioData,
        event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<TranscriptionOutput, String> {
        if profiling::enabled() {
            profiling::log_point(
                "openai.streaming_request",
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

        self.transcribe_streaming_with_part(part, None, event_tx)
            .await
    }

    /// 共通の転写処理
    async fn transcribe_with_part(
        &self,
        file_part: multipart::Part,
        prompt: Option<&str>,
    ) -> Result<TranscriptionOutput, String> {
        let overall_timer = profiling::Timer::start("openai.transcribe_total");
        let url = "https://api.openai.com/v1/audio/transcriptions";

        // multipart/form-data
        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("language", "ja")
            .text("include[]", "logprobs");

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
        Ok(TranscriptionOutput {
            text: transcription.text,
            tokens: map_logprobs(transcription.logprobs),
        })
    }

    async fn transcribe_streaming_with_part(
        &self,
        file_part: multipart::Part,
        prompt: Option<&str>,
        event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<TranscriptionOutput, String> {
        let overall_timer = profiling::Timer::start("openai.streaming_transcribe_total");
        let url = "https://api.openai.com/v1/audio/transcriptions";

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("language", "ja")
            .text("stream", "true")
            .text("include[]", "logprobs");

        if let Some(prompt_text) = prompt {
            let formatted_prompt = format!(
                "The following text provides relevant context. Please consider this when creating the transcription: {:?}",
                prompt_text
            );
            form = form.text("prompt", formatted_prompt);
        }

        let send_timer = profiling::Timer::start("openai.streaming_send");
        let mut response = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {}", e))?;
        send_timer.log();

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .map_err(|e| format!("Failed to read response: {}", e))?;
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

        let mut parser = StreamingEventParser::default();
        let mut final_output = None;

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("Failed to read response chunk: {}", e))?
        {
            for event in parser.push_chunk(&chunk)? {
                match event {
                    StreamingTranscriptionEvent::Delta(delta) => {
                        let _ = event_tx.send(TranscriptionEvent::Delta(delta));
                    }
                    StreamingTranscriptionEvent::Completed(output) => {
                        final_output = Some(output);
                    }
                }
            }
        }

        for event in parser.finish()? {
            match event {
                StreamingTranscriptionEvent::Delta(delta) => {
                    let _ = event_tx.send(TranscriptionEvent::Delta(delta));
                }
                StreamingTranscriptionEvent::Completed(output) => {
                    final_output = Some(output);
                }
            }
        }

        let output = final_output.ok_or("Streaming response completed without final text")?;

        if profiling::enabled() {
            overall_timer.log_with(&format!("status={} text_len={}", status, output.text.len()));
        } else {
            overall_timer.log();
        }

        Ok(output)
    }
}

#[derive(Default)]
struct StreamingEventParser {
    buffer: Vec<u8>,
}

impl StreamingEventParser {
    fn push_chunk(&mut self, chunk: &[u8]) -> Result<Vec<StreamingTranscriptionEvent>, String> {
        self.buffer.extend_from_slice(chunk);
        self.drain_complete_events()
    }

    fn finish(&mut self) -> Result<Vec<StreamingTranscriptionEvent>, String> {
        if self.buffer.iter().all(|byte| byte.is_ascii_whitespace()) {
            return Ok(Vec::new());
        }

        let remainder = std::mem::take(&mut self.buffer);
        parse_streaming_frame(&remainder).map(|event| event.into_iter().collect())
    }

    fn drain_complete_events(&mut self) -> Result<Vec<StreamingTranscriptionEvent>, String> {
        let mut events = Vec::new();

        while let Some((separator, separator_len)) = find_frame_separator(&self.buffer) {
            let frame = self.buffer[..separator].to_vec();
            self.buffer.drain(..separator + separator_len);
            if let Some(event) = parse_streaming_frame(&frame)? {
                events.push(event);
            }
        }

        Ok(events)
    }
}

#[cfg(test)]
fn parse_streaming_events(body: &str) -> Result<Vec<StreamingTranscriptionEvent>, String> {
    let mut parser = StreamingEventParser::default();
    let mut events = parser.push_chunk(body.as_bytes())?;
    events.extend(parser.finish()?);
    Ok(events)
}

fn parse_streaming_frame(frame: &[u8]) -> Result<Option<StreamingTranscriptionEvent>, String> {
    let normalized = std::str::from_utf8(frame)
        .map_err(|e| format!("Failed to decode streaming frame: {}", e))?
        .replace("\r\n", "\n");
    let mut event_name = None;
    let mut data_lines = Vec::new();

    for line in normalized.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event_name = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            let data = value.trim();
            if data == "[DONE]" {
                return Ok(None);
            }
            data_lines.push(data);
        }
    }

    let data = data_lines.join("\n");
    if data.is_empty() {
        return Ok(None);
    }

    let envelope: StreamingEventEnvelope = serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse streaming event payload: {}", e))?;
    let Some(event_name) = event_name.or(envelope.event_type.clone()) else {
        return Ok(None);
    };

    match event_name.as_str() {
        "transcript.text.delta" => match envelope.delta {
            Some(delta) => Ok(Some(StreamingTranscriptionEvent::Delta(delta))),
            None => {
                let payload: StreamingDeltaResponse = serde_json::from_str(&data)
                    .map_err(|e| format!("Failed to parse streaming delta: {}", e))?;
                Ok(Some(StreamingTranscriptionEvent::Delta(payload.delta)))
            }
        },
        "transcript.text.done" => match envelope.text {
            Some(text) => Ok(Some(StreamingTranscriptionEvent::Completed(
                TranscriptionOutput {
                    text,
                    tokens: map_logprobs(envelope.logprobs.unwrap_or_default()),
                },
            ))),
            None => {
                let payload: StreamingCompletedResponse = serde_json::from_str(&data)
                    .map_err(|e| format!("Failed to parse streaming completion: {}", e))?;
                Ok(Some(StreamingTranscriptionEvent::Completed(
                    TranscriptionOutput {
                        text: payload.text,
                        tokens: map_logprobs(payload.logprobs),
                    },
                )))
            }
        },
        _ => Ok(None),
    }
}

fn map_logprobs(logprobs: Vec<TokenLogprobResponse>) -> Vec<TranscriptionToken> {
    logprobs
        .into_iter()
        .map(|token| TranscriptionToken::new(token.token, token.logprob))
        .collect()
}

fn find_frame_separator(buffer: &[u8]) -> Option<(usize, usize)> {
    for (index, window) in buffer.windows(2).enumerate() {
        if window == b"\n\n" {
            return Some((index, 2));
        }
    }

    for (index, window) in buffer.windows(4).enumerate() {
        if window == b"\r\n\r\n" {
            return Some((index, 4));
        }
    }

    None
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
        if std::env::var("OPENAI_API_KEY").is_ok()
            || EnvConfig::get().transcription.openai_api_key.is_some()
        {
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
        if EnvConfig::get().transcription.openai_api_key.is_none()
            && std::env::var("OPENAI_API_KEY").is_err()
        {
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
        if EnvConfig::get().transcription.openai_api_key.is_none()
            && std::env::var("OPENAI_API_KEY").is_err()
        {
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

    /// ストリーミングレスポンスのdeltaを受信順に連結できる
    #[test]
    fn streaming_deltas_can_be_concatenated_in_received_order() {
        let body = concat!(
            "data: {\"type\":\"transcript.text.delta\",\"delta\":\"こん\"}\n\n",
            "data: {\"type\":\"transcript.text.delta\",\"delta\":\"にちは\"}\n\n",
            "data: {\"type\":\"transcript.text.done\",\"text\":\"こんにちは\"}\n\n"
        );

        let events = parse_streaming_events(body).expect("stream should parse");

        assert_eq!(
            events,
            vec![
                StreamingTranscriptionEvent::Delta("こん".to_string()),
                StreamingTranscriptionEvent::Delta("にちは".to_string()),
                StreamingTranscriptionEvent::Completed(TranscriptionOutput::from_text(
                    "こんにちは".to_string(),
                )),
            ]
        );
    }

    /// CRLF 区切りのストリーミングレスポンスも逐次パースできる
    #[test]
    fn streaming_events_support_crlf_delimiters() {
        let body = concat!(
            "data: {\"type\":\"transcript.text.delta\",\"delta\":\"こん\"}\r\n\r\n",
            "data: {\"type\":\"transcript.text.done\",\"text\":\"こんにちは\"}\r\n\r\n"
        );

        let events = parse_streaming_events(body).expect("stream should parse");

        assert_eq!(
            events,
            vec![
                StreamingTranscriptionEvent::Delta("こん".to_string()),
                StreamingTranscriptionEvent::Completed(TranscriptionOutput::from_text(
                    "こんにちは".to_string(),
                )),
            ]
        );
    }

    /// UTF-8の途中バイトで分割されても完成フレーム単位でパースできる
    #[test]
    fn streaming_parser_handles_multibyte_utf8_split_across_chunks() {
        let mut parser = StreamingEventParser::default();
        let utf8_bytes = "data: {\"type\":\"transcript.text.delta\",\"delta\":\"こ".as_bytes();
        let multibyte_tail = "ん\"}\n\n".as_bytes();

        let events = parser
            .push_chunk(utf8_bytes)
            .expect("first chunk should buffer");
        assert!(events.is_empty());

        let events = parser
            .push_chunk(multibyte_tail)
            .expect("second chunk should parse");
        assert_eq!(
            events,
            vec![StreamingTranscriptionEvent::Delta("こん".to_string())]
        );
    }

    /// 区切り文字がchunkをまたいでもイベント境界を認識できる
    #[test]
    fn streaming_parser_handles_split_crlf_frame_separator() {
        let mut parser = StreamingEventParser::default();

        let first = parser
            .push_chunk(
                "data: {\"type\":\"transcript.text.delta\",\"delta\":\"こん\"}\r".as_bytes(),
            )
            .expect("first chunk should buffer");
        assert!(first.is_empty());

        let second = parser
            .push_chunk(
                "\n\r\ndata: {\"type\":\"transcript.text.done\",\"text\":\"こんにちは\"}\r\n\r\n"
                    .as_bytes(),
            )
            .expect("second chunk should parse");

        assert_eq!(
            second,
            vec![
                StreamingTranscriptionEvent::Delta("こん".to_string()),
                StreamingTranscriptionEvent::Completed(TranscriptionOutput::from_text(
                    "こんにちは".to_string(),
                )),
            ]
        );
    }

    /// 旧来のeventヘッダ付きレスポンスも後方互換でパースできる
    #[test]
    fn streaming_parser_supports_legacy_event_header_format() {
        let body = concat!(
            "event: transcript.text.delta\n",
            "data: {\"delta\":\"こん\"}\n\n",
            "event: transcript.text.done\n",
            "data: {\"text\":\"こんにちは\"}\n\n"
        );

        let events = parse_streaming_events(body).expect("legacy stream should parse");

        assert_eq!(
            events,
            vec![
                StreamingTranscriptionEvent::Delta("こん".to_string()),
                StreamingTranscriptionEvent::Completed(TranscriptionOutput::from_text(
                    "こんにちは".to_string(),
                )),
            ]
        );
    }

    /// 完了イベントにlogprobsが含まれる場合はトークン情報へ変換できる
    #[test]
    fn streaming_completion_maps_logprobs_to_tokens() {
        let body = concat!(
            "data: {\"type\":\"transcript.text.done\",\"text\":\"こんにちは\",\"logprobs\":[",
            "{\"token\":\"こん\",\"logprob\":-0.2},",
            "{\"token\":\"にちは\",\"logprob\":-0.7}",
            "]}\n\n"
        );

        let events = parse_streaming_events(body).expect("stream should parse");

        assert_eq!(
            events,
            vec![StreamingTranscriptionEvent::Completed(
                TranscriptionOutput {
                    text: "こんにちは".to_string(),
                    tokens: vec![
                        TranscriptionToken::new("こん", -0.2),
                        TranscriptionToken::new("にちは", -0.7),
                    ],
                }
            )]
        );
    }
}
