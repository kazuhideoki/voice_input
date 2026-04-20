//! mlx-qwen3-asr CLI のアダプター実装
//! Application層のTranscriptionClientトレイトを実装

use crate::application::AudioData;
use crate::application::{TranscriptionClient, TranscriptionClientError};
use crate::domain::transcription::TranscriptionOutput;
use crate::error::Result;
use crate::utils::config::{EnvConfig, TranscriptionConfig, resolve_mlx_qwen3_asr_command};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
enum MlxQwen3AsrError {
    #[error("failed to create temporary audio file: {0}")]
    TempFileCreate(#[source] std::io::Error),
    #[error("failed to execute mlx-qwen3-asr command: {0}")]
    CommandExecution(#[source] std::io::Error),
    #[error("mlx-qwen3-asr exited with status {status}: {message}")]
    CommandStatus { status: i32, message: String },
    #[error("mlx-qwen3-asr returned empty transcription output")]
    EmptyOutput,
}

/// mlx-qwen3-asr CLI のアダプター
pub struct MlxQwen3AsrTranscriptionAdapter {
    command: String,
    model: String,
}

impl MlxQwen3AsrTranscriptionAdapter {
    /// 現在の環境設定から新しいアダプターを作成
    pub fn new() -> Self {
        Self::from_config(&EnvConfig::get().transcription)
    }

    /// 転写設定から新しいアダプターを作成
    pub fn from_config(config: &TranscriptionConfig) -> Self {
        Self {
            command: resolve_mlx_qwen3_asr_command(&config.mlx_qwen3_asr_command),
            model: config.model.clone(),
        }
    }

    async fn transcribe_audio(&self, audio: AudioData) -> Result<TranscriptionOutput> {
        let temp_file = TempAudioFile::create(&audio)
            .map_err(|error| map_init_error(MlxQwen3AsrError::TempFileCreate(error)))?;

        let output = Command::new(&self.command)
            .arg(temp_file.path())
            .arg("--model")
            .arg(&self.model)
            .arg("--stdout-only")
            .arg("--no-progress")
            .output()
            .await
            .map_err(|error| map_request_error(MlxQwen3AsrError::CommandExecution(error)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let message = if !stderr.is_empty() { stderr } else { stdout };
            return Err(map_request_error(MlxQwen3AsrError::CommandStatus {
                status: output.status.code().unwrap_or(-1),
                message,
            }));
        }

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            return Err(map_request_error(MlxQwen3AsrError::EmptyOutput));
        }

        Ok(TranscriptionOutput::from_text(text))
    }
}

impl Default for MlxQwen3AsrTranscriptionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TranscriptionClient for MlxQwen3AsrTranscriptionAdapter {
    async fn transcribe(&self, audio: AudioData, _language: &str) -> Result<TranscriptionOutput> {
        self.transcribe_audio(audio).await
    }
}

fn map_init_error(error: MlxQwen3AsrError) -> crate::error::VoiceInputError {
    crate::error::VoiceInputError::from(TranscriptionClientError::Initialization {
        message: error.to_string(),
    })
}

fn map_request_error(error: MlxQwen3AsrError) -> crate::error::VoiceInputError {
    crate::error::VoiceInputError::from(TranscriptionClientError::Request {
        message: error.to_string(),
    })
}

struct TempAudioFile {
    path: PathBuf,
}

impl TempAudioFile {
    fn create(audio: &AudioData) -> std::io::Result<Self> {
        let extension = file_extension(audio);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "voice_input_mlx_{}_{}.{}",
            std::process::id(),
            unique,
            extension
        ));
        std::fs::write(&path, &audio.bytes)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempAudioFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn file_extension(audio: &AudioData) -> &'static str {
    match Path::new(&audio.file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("wav") => "wav",
        Some("flac") => "flac",
        _ if audio.mime_type == "audio/flac" => "flac",
        _ => "wav",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    struct Fixture {
        _temp_dir: TempDir,
        script_path: PathBuf,
    }

    impl Fixture {
        fn new(script_body: &str) -> Self {
            let temp_dir = TempDir::new().expect("create temp dir");
            let script_path = temp_dir.path().join("mlx-qwen3-asr");
            fs::write(&script_path, script_body).expect("write fake script");
            let mut permissions = fs::metadata(&script_path)
                .expect("read metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&script_path, permissions).expect("set executable");

            Self {
                _temp_dir: temp_dir,
                script_path,
            }
        }

        fn adapter(&self) -> MlxQwen3AsrTranscriptionAdapter {
            MlxQwen3AsrTranscriptionAdapter {
                command: self.script_path.display().to_string(),
                model: "Qwen/Qwen3-ASR-1.7B".to_string(),
            }
        }
    }

    fn sample_audio_data() -> AudioData {
        AudioData {
            bytes: b"RIFF".to_vec(),
            mime_type: "audio/wav",
            file_name: "sample.wav".to_string(),
        }
    }

    /// CLI が標準出力へ返した文字列を転写結果として扱える
    #[tokio::test]
    async fn cli_stdout_can_be_used_as_transcription_text() {
        let fixture = Fixture::new(
            r#"#!/bin/sh
printf "音声テキスト"
"#,
        );

        let result = fixture
            .adapter()
            .transcribe(sample_audio_data(), "ja")
            .await
            .expect("transcription should succeed");

        assert_eq!(result, TranscriptionOutput::from_text("音声テキスト"));
    }

    /// 実CLI互換の stdout-only 指定で標準出力から転写結果を受け取れる
    #[tokio::test]
    async fn cli_requests_stdout_only_output() {
        let fixture = Fixture::new(
            r#"#!/bin/sh
for arg in "$@"; do
    if [ "$arg" = "--stdout-only" ]; then
        printf "標準出力の結果"
        exit 0
    fi
done

touch "$(dirname "$1")/voice_input_mlx_sample.txt"
exit 0
"#,
        );

        let result = fixture
            .adapter()
            .transcribe(sample_audio_data(), "ja")
            .await
            .expect("transcription should succeed");

        assert_eq!(result, TranscriptionOutput::from_text("標準出力の結果"));
    }

    /// CLI が失敗した場合は転写エラーとして返す
    #[tokio::test]
    async fn cli_failure_is_returned_as_request_error() {
        let fixture = Fixture::new(
            r#"#!/bin/sh
echo "cli failed" >&2
exit 1
"#,
        );

        let error = fixture
            .adapter()
            .transcribe(sample_audio_data(), "ja")
            .await
            .expect_err("transcription should fail");

        assert!(error.to_string().contains("cli failed"));
    }
}
