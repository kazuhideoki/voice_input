//! OpenAI STT API ラッパ。
//! WAV ファイルを multipart/form-data で転写エンドポイントに送信します。
use reqwest::multipart;
use serde::Deserialize;
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// STT API のレスポンス JSON。
#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    pub text: String,
}

/// WAV ファイルを STT API で文字起こしします。
///
/// * `audio_file_path` – 入力 WAV ファイルパス
/// * `prompt`           – コンテキストプロンプト (任意)
///
/// STT モデルは `OPENAI_TRANSCRIBE_MODEL` が存在しない場合 `gpt-4o-mini-transcribe` を使用します。
pub async fn transcribe_audio(
    audio_file_path: &str,
    prompt: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let api_key =
        env::var("OPENAI_API_KEY").map_err(|_| "OPENAI_API_KEY environment variable not set")?;

    let model = env::var("OPENAI_TRANSCRIBE_MODEL")
        .unwrap_or_else(|_| "gpt-4o-mini-transcribe".to_string());

    let client = reqwest::Client::new();
    let url = "https://api.openai.com/v1/audio/transcriptions";

    // TODO ファイルを作成せずにオンメモリで試す
    // ---- ファイル読み込み ------------------------------------------------
    let path = Path::new(audio_file_path);
    let file_name = path
        .file_name()
        .ok_or("Invalid file path")?
        .to_string_lossy()
        .into_owned();

    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let file_part = multipart::Part::bytes(buffer)
        .file_name(file_name)
        .mime_str("audio/wav")?;

    // ---- multipart/form-data -------------------------------------------
    let mut form = multipart::Form::new()
        .part("file", file_part)
        .text("model", model)
        .text("language", "ja");

    if let Some(prompt_text) = prompt {
        let formatted_prompt = format!(
            "The following text provides relevant context. Please consider this when creating the transcription: {:?}",
            prompt_text
        );
        form = form.text("prompt", formatted_prompt);
    }

    // ---- 送信 -----------------------------------------------------------
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    if !status.is_success() {
        return Err(format!("API request failed with status {}: {}", status, body).into());
    }

    let transcription: TranscriptionResponse = serde_json::from_str(&body)?;
    Ok(transcription.text)
}

// === Unit tests ==========================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_transcription_response_json() {
        let json = r#"{"text":"こんにちは"}"#;
        let resp: TranscriptionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.text, "こんにちは");
    }
}
