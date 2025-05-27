//! Unix Domain Socket (UDS) ベースのシンプルな IPC モジュール。
//! `voice_input` CLI ↔ `voice_inputd` デーモン間の通信で利用します。
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

/// デーモンソケットパスを返します。
pub fn socket_path() -> PathBuf {
    let dir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(dir).join("voice_input.sock")
}

/// CLI からデーモンへ送るコマンド列挙。
#[derive(Debug, Serialize, Deserialize)]
pub enum IpcCmd {
    /// 録音開始
    Start {
        paste: bool,
        prompt: Option<String>,
        direct_input: bool,
    },
    /// 録音停止
    Stop,
    /// 録音トグル
    Toggle {
        paste: bool,
        prompt: Option<String>,
        direct_input: bool,
    },
    /// ステータス取得
    Status,
    ListDevices,
    Health,
}

/// デーモンからの汎用レスポンス。
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResp {
    pub ok: bool,
    pub msg: String,
}

/// シリアライズ可能な音声データ
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AudioDataDto {
    Memory(Vec<u8>),
}

/// 録音結果を表す構造体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingResult {
    pub audio_data: AudioDataDto,
    pub duration_ms: u64,
}

use crate::infrastructure::audio::cpal_backend::AudioData;

impl From<AudioData> for AudioDataDto {
    fn from(data: AudioData) -> Self {
        match data {
            AudioData::Memory(bytes) => AudioDataDto::Memory(bytes),
            AudioData::File(path) => match std::fs::read(&path) {
                Ok(bytes) => AudioDataDto::Memory(bytes),
                Err(e) => {
                    eprintln!("failed to read audio file {path:?}: {e}");
                    AudioDataDto::Memory(Vec::new())
                }
            },
        }
    }
}

impl From<AudioDataDto> for AudioData {
    fn from(dto: AudioDataDto) -> Self {
        match dto {
            AudioDataDto::Memory(bytes) => AudioData::Memory(bytes),
        }
    }
}

/// コマンドを送信して `IpcResp` を取得する同期ユーティリティ。
pub fn send_cmd(cmd: &IpcCmd) -> Result<IpcResp, Box<dyn Error>> {
    use futures::{SinkExt, StreamExt};
    use tokio::net::UnixStream;
    use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let path = socket_path();
            if !Path::new(&path).exists() {
                return Err("daemon socket not found".into());
            }

            let stream = UnixStream::connect(path).await?;
            let (r, w) = stream.into_split();
            let mut writer = FramedWrite::new(w, LinesCodec::new());
            let mut reader = FramedRead::new(r, LinesCodec::new());

            writer.send(serde_json::to_string(cmd)?).await?;
            if let Some(Ok(line)) = reader.next().await {
                Ok(serde_json::from_str::<IpcResp>(&line)?)
            } else {
                Err("no response from daemon".into())
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_data_dto_memory_variant() {
        let wav_data = vec![0u8, 1, 2, 3, 4, 5];
        let audio_data = AudioDataDto::Memory(wav_data.clone());

        let AudioDataDto::Memory(data) = audio_data;
        assert_eq!(data, wav_data);
    }

    #[test]
    fn test_recording_result_creation() {
        let audio_data = AudioDataDto::Memory(vec![1, 2, 3]);
        let duration_ms = 1500u64;

        let result = RecordingResult {
            audio_data: audio_data.clone(),
            duration_ms,
        };

        assert_eq!(result.duration_ms, 1500);
        let AudioDataDto::Memory(data) = result.audio_data;
        assert_eq!(data, vec![1, 2, 3]);
    }

    #[test]
    fn test_audio_data_dto_serialization() {
        let memory_data = AudioDataDto::Memory(vec![1, 2, 3, 4, 5]);
        let json = serde_json::to_string(&memory_data).unwrap();
        let deserialized: AudioDataDto = serde_json::from_str(&json).unwrap();

        let AudioDataDto::Memory(data) = deserialized;
        assert_eq!(data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_recording_result_serialization() {
        let result = RecordingResult {
            audio_data: AudioDataDto::Memory(vec![10, 20, 30]),
            duration_ms: 2500,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: RecordingResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.duration_ms, 2500);
        let AudioDataDto::Memory(data) = deserialized.audio_data;
        assert_eq!(data, vec![10, 20, 30]);
    }

    #[test]
    fn test_from_audio_data_to_dto() {
        // Test Memory variant
        let audio_data = AudioData::Memory(vec![1, 2, 3, 4]);
        let dto: AudioDataDto = audio_data.into();
        let AudioDataDto::Memory(data) = dto;
        assert_eq!(data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_from_dto_to_audio_data() {
        // Test Memory variant
        let dto = AudioDataDto::Memory(vec![5, 6, 7, 8]);
        let audio_data: AudioData = dto.into();
        if let AudioData::Memory(data) = audio_data {
            assert_eq!(data, vec![5, 6, 7, 8]);
        } else {
            panic!("Expected Memory variant");
        }
    }

    #[test]
    fn test_ipc_compatibility() {
        // Test that existing IPC commands still work
        let cmd = IpcCmd::Start {
            paste: true,
            prompt: Some("test prompt".to_string()),
            direct_input: false,
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

        match deserialized {
            IpcCmd::Start {
                paste,
                prompt,
                direct_input,
            } => {
                assert!(paste);
                assert_eq!(prompt, Some("test prompt".to_string()));
                assert!(!direct_input);
            }
            _ => panic!("Expected Start command"),
        }

        // Test IpcResp compatibility
        let resp = IpcResp {
            ok: true,
            msg: "Success".to_string(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: IpcResp = serde_json::from_str(&json).unwrap();

        assert!(deserialized.ok);
        assert_eq!(deserialized.msg, "Success");
    }
}
