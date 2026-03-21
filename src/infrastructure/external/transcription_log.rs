use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::application::{TranscriptionLogEntry, TranscriptionLogWriter};
use crate::error::{Result, VoiceInputError};

const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

/// 転写ログを専用スレッドでJSONファイルへ保存する
pub struct NonBlockingTranscriptionLogWriter {
    sender: mpsc::SyncSender<TranscriptionLogEntry>,
}

impl NonBlockingTranscriptionLogWriter {
    /// 非同期保存ワーカーを起動する
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::with_capacity(path, DEFAULT_CHANNEL_CAPACITY)
    }

    /// 非同期保存ワーカーを起動する
    pub fn with_capacity(path: impl Into<PathBuf>, capacity: usize) -> Self {
        let path = path.into();
        let (sender, receiver) = mpsc::sync_channel::<TranscriptionLogEntry>(capacity);

        std::thread::Builder::new()
            .name("transcription-log-writer".to_string())
            .spawn(move || {
                while let Ok(entry) = receiver.recv() {
                    if let Err(error) = append_log_entry(&path, entry) {
                        eprintln!("Failed to write transcription log: {}", error);
                    }
                }
            })
            .expect("transcription log writer thread should start");

        Self { sender }
    }
}

impl TranscriptionLogWriter for NonBlockingTranscriptionLogWriter {
    fn enqueue(&self, entry: TranscriptionLogEntry) -> Result<()> {
        self.sender.try_send(entry).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => {
                VoiceInputError::SystemError("Transcription log worker queue is full".to_string())
            }
            mpsc::TrySendError::Disconnected(_) => {
                VoiceInputError::SystemError("Transcription log worker channel closed".to_string())
            }
        })
    }
}

fn append_log_entry(path: &Path, entry: TranscriptionLogEntry) -> std::result::Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create log directory: {}", error))?;
        }
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("Failed to open transcription log: {}", error))?;
    let content = serde_json::to_vec(&entry)
        .map_err(|error| format!("Failed to serialize transcription log entry: {}", error))?;
    file.write_all(&content)
        .map_err(|error| format!("Failed to write transcription log: {}", error))?;
    file.write_all(b"\n")
        .map_err(|error| format!("Failed to terminate transcription log line: {}", error))?;
    file.flush()
        .map_err(|error| format!("Failed to flush transcription log: {}", error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// 保存要求を送ると別スレッドでJSON Linesへ追記される
    #[test]
    fn non_blocking_writer_appends_entries_to_jsonl_file() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("transcription-log.jsonl");
        let writer = NonBlockingTranscriptionLogWriter::new(&path);

        writer
            .enqueue(TranscriptionLogEntry {
                recorded_at: "2026-03-20T10:00:00+09:00".to_string(),
                raw_text: "生テキスト".to_string(),
                processed_text: "処理済みテキスト".to_string(),
                tokens: vec![crate::domain::transcription::TranscriptionToken::new(
                    "生", -0.4,
                )],
            })
            .unwrap();

        for _ in 0..20 {
            if path.exists() {
                let content = fs::read_to_string(&path).unwrap();
                if content.contains("処理済みテキスト") {
                    let logs = content
                        .lines()
                        .map(|line| serde_json::from_str::<TranscriptionLogEntry>(line).unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(logs.len(), 1);
                    assert_eq!(logs[0].raw_text, "生テキスト");
                    assert_eq!(logs[0].processed_text, "処理済みテキスト");
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        panic!("log file was not written in time");
    }

    /// 既存の壊れた行があっても末尾へ新規ログを追記できる
    #[test]
    fn non_blocking_writer_appends_even_when_existing_line_is_invalid() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("transcription-log.jsonl");
        fs::write(&path, "{\"broken\":true\n").unwrap();

        let writer = NonBlockingTranscriptionLogWriter::new(&path);
        writer
            .enqueue(TranscriptionLogEntry {
                recorded_at: "2026-03-20T10:00:01+09:00".to_string(),
                raw_text: "追加前".to_string(),
                processed_text: "追加後".to_string(),
                tokens: vec![crate::domain::transcription::TranscriptionToken::new(
                    "追加", -0.2,
                )],
            })
            .unwrap();

        for _ in 0..20 {
            let content = fs::read_to_string(&path).unwrap();
            if content.contains("追加後") {
                let last_line = content.lines().last().unwrap();
                let entry: TranscriptionLogEntry = serde_json::from_str(last_line).unwrap();
                assert_eq!(entry.processed_text, "追加後");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        panic!("log file was not appended in time");
    }
}
