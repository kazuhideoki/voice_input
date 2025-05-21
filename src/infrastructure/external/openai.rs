//! OpenAI STT API ラッパ。
//! WAV ファイルを multipart/form-data で転写エンドポイントに送信します。
use reqwest::multipart;
use serde::Deserialize;
use std::env;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

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

    let mut file = File::open(path).await?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).await?;

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

/// Suggest dictionary candidate entries using the ChatGPT API.
///
/// The model is taken from `OPENAI_DICT_MODEL` or defaults to `gpt-4o`.
pub async fn suggest_dict_candidates(
    text: &str,
) -> Result<Vec<WordSuggestion>, Box<dyn std::error::Error>> {
    let api_key =
        env::var("OPENAI_API_KEY").map_err(|_| "OPENAI_API_KEY environment variable not set")?;

    let model = env::var("OPENAI_DICT_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());

    #[derive(Deserialize)]
    struct ChatResponse {
        choices: Vec<Choice>,
    }

    #[derive(Deserialize)]
    struct Choice {
        message: Message,
    }

    #[derive(Deserialize)]
    struct Message {
        content: String,
    }

    let client = reqwest::Client::new();
    let url = "https://api.openai.com/v1/chat/completions";

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": "Extract dictionary candidates from the given text. Respond with a JSON array of objects each having surface and replacement fields. Use Japanese."},
            {"role": "user", "content": text}
        ],
        "temperature": 0.0
    });

    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(format!("API request failed with status {}: {}", status, body).into());
    }

    let chat: ChatResponse = serde_json::from_str(&body)?;
    let content = chat
        .choices
        .get(0)
        .ok_or("no choices")?
        .message
        .content
        .trim()
        .to_string();

    let suggestions: Vec<WordSuggestion> = serde_json::from_str(&content).unwrap_or_default();
    Ok(suggestions)
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
