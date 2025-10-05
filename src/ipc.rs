//! Unix Domain Socket (UDS) ベースのシンプルな IPC モジュール。
//! `voice_input` CLI ↔ `voice_inputd` デーモン間の通信で利用します。
use crate::domain::stack::StackInfo;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

const SOCKET_FILENAME: &str = "voice_input.sock";
const DEFAULT_SOCKET_PATH: &str = "/tmp/voice_input.sock";

/// デーモンソケットパスを返します。
pub fn socket_path() -> PathBuf {
    if let Some(path) = socket_env("VOICE_INPUT_SOCKET_PATH") {
        return PathBuf::from(path);
    }

    if let Some(dir) = socket_env("VOICE_INPUT_SOCKET_DIR") {
        return PathBuf::from(dir).join(SOCKET_FILENAME);
    }

    PathBuf::from(DEFAULT_SOCKET_PATH)
}

/// CLI からデーモンへ送るコマンド列挙。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// スタックモードを有効化
    EnableStackMode,
    /// スタックモードを無効化
    DisableStackMode,
    /// 指定番号のスタックをペースト
    PasteStack {
        number: u32,
    },
    /// スタック一覧を取得
    ListStacks,
    /// 全スタックをクリア
    ClearStacks,
}

/// デーモンからの汎用レスポンス。
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResp {
    pub ok: bool,
    pub msg: String,
}

/// シリアライズ可能な音声データ（メモリモード専用）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AudioDataDto(pub Vec<u8>);

/// 録音結果を表す構造体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingResult {
    pub audio_data: AudioDataDto,
    pub duration_ms: u64,
}

/// スタック関連のレスポンス
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcStackResp {
    pub stacks: Vec<StackInfo>,
    pub mode_enabled: bool,
}

use crate::infrastructure::audio::cpal_backend::AudioData;

impl From<AudioData> for AudioDataDto {
    fn from(data: AudioData) -> Self {
        AudioDataDto(data.bytes)
    }
}

impl From<AudioDataDto> for AudioData {
    fn from(dto: AudioDataDto) -> Self {
        // 簡易判定: FLAC マジックヘッダ "fLaC"
        let mime = if dto.0.starts_with(&[0x66, 0x4C, 0x61, 0x43]) {
            ("audio/flac", "audio.flac")
        } else {
            ("audio/wav", "audio.wav")
        };
        AudioData { bytes: dto.0, mime_type: mime.0, file_name: mime.1.to_string() }
    }
}

fn socket_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
    use std::sync::Mutex;

    static SOCKET_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env_lock<F: FnOnce()>(f: F) {
        let _guard = SOCKET_ENV_LOCK.lock().unwrap();
        f();
    }

    fn store_env(key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn set_env(key: &str, value: &str) {
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn remove_env(key: &str) {
        unsafe {
            std::env::remove_var(key);
        }
    }

    fn restore_env(key: &str, value: Option<String>) {
        if let Some(val) = value {
            set_env(key, &val);
        } else {
            remove_env(key);
        }
    }

    #[test]
    fn test_socket_path_default() {
        with_env_lock(|| {
            let orig_path = store_env("VOICE_INPUT_SOCKET_PATH");
            let orig_dir = store_env("VOICE_INPUT_SOCKET_DIR");
            remove_env("VOICE_INPUT_SOCKET_PATH");
            remove_env("VOICE_INPUT_SOCKET_DIR");

            assert_eq!(socket_path(), PathBuf::from(DEFAULT_SOCKET_PATH));

            restore_env("VOICE_INPUT_SOCKET_PATH", orig_path);
            restore_env("VOICE_INPUT_SOCKET_DIR", orig_dir);
        });
    }

    #[test]
    fn test_socket_path_env_override() {
        with_env_lock(|| {
            let orig_path = store_env("VOICE_INPUT_SOCKET_PATH");
            let orig_dir = store_env("VOICE_INPUT_SOCKET_DIR");
            set_env("VOICE_INPUT_SOCKET_PATH", "/tmp/custom.sock");
            remove_env("VOICE_INPUT_SOCKET_DIR");

            assert_eq!(socket_path(), PathBuf::from("/tmp/custom.sock"));

            restore_env("VOICE_INPUT_SOCKET_PATH", orig_path);
            restore_env("VOICE_INPUT_SOCKET_DIR", orig_dir);
        });
    }

    #[test]
    fn test_socket_dir_env_override() {
        with_env_lock(|| {
            let orig_path = store_env("VOICE_INPUT_SOCKET_PATH");
            let orig_dir = store_env("VOICE_INPUT_SOCKET_DIR");
            remove_env("VOICE_INPUT_SOCKET_PATH");
            set_env("VOICE_INPUT_SOCKET_DIR", "/var/tmp");

            assert_eq!(
                socket_path(),
                PathBuf::from("/var/tmp").join(SOCKET_FILENAME)
            );

            restore_env("VOICE_INPUT_SOCKET_PATH", orig_path);
            restore_env("VOICE_INPUT_SOCKET_DIR", orig_dir);
        });
    }

    #[test]
    fn test_audio_data_dto_struct() {
        let wav_data = vec![0u8, 1, 2, 3, 4, 5];
        let audio_data = AudioDataDto(wav_data.clone());

        assert_eq!(audio_data.0, wav_data);
    }

    #[test]
    fn test_audio_data_dto_serde() {
        let wav_data = vec![0u8, 1, 2, 3, 4, 5];
        let audio_data = AudioDataDto(wav_data.clone());

        let json = serde_json::to_string(&audio_data).unwrap();
        let deserialized: AudioDataDto = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.0, wav_data);
    }

    #[test]
    fn test_recording_result_creation() {
        let audio_data = AudioDataDto(vec![1, 2, 3]);
        let duration_ms = 1500u64;

        let result = RecordingResult {
            audio_data: audio_data.clone(),
            duration_ms,
        };

        assert_eq!(result.duration_ms, 1500);
        assert_eq!(result.audio_data.0, vec![1, 2, 3]);
    }

    #[test]
    fn test_recording_result_serialization() {
        let audio_data = AudioDataDto(vec![10, 20, 30]);
        let duration_ms = 3000u64;

        let result = RecordingResult {
            audio_data,
            duration_ms,
        };

        assert_eq!(result.duration_ms, 3000);
        assert_eq!(result.audio_data.0, vec![10, 20, 30]);
    }

    #[test]
    fn test_json_serialization() {
        let data = AudioDataDto(vec![1, 2, 3, 4, 5]);
        let json = serde_json::to_string(&data).unwrap();
        let deserialized: AudioDataDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.0, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_recording_result_json() {
        let result = RecordingResult {
            audio_data: AudioDataDto(vec![10, 20, 30]),
            duration_ms: 2500,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: RecordingResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.duration_ms, 2500);
        assert_eq!(deserialized.audio_data.0, vec![10, 20, 30]);
    }

    #[test]
    fn test_from_audio_data_to_dto() {
        let audio_data = AudioData { bytes: vec![1, 2, 3, 4], mime_type: "audio/wav", file_name: "audio.wav".to_string() };
        let dto: AudioDataDto = audio_data.into();
        assert_eq!(dto.0, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_from_dto_to_audio_data() {
        let dto = AudioDataDto(vec![5, 6, 7, 8]);
        let audio_data: AudioData = dto.into();
        assert_eq!(audio_data.bytes, vec![5, 6, 7, 8]);
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

    #[test]
    fn test_stack_mode_commands_serialization() {
        // EnableStackMode
        let cmd = IpcCmd::EnableStackMode;
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, IpcCmd::EnableStackMode));

        // DisableStackMode
        let cmd = IpcCmd::DisableStackMode;
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, IpcCmd::DisableStackMode));

        // PasteStack
        let cmd = IpcCmd::PasteStack { number: 3 };
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();
        match deserialized {
            IpcCmd::PasteStack { number } => assert_eq!(number, 3),
            _ => panic!("Expected PasteStack command"),
        }

        // ListStacks
        let cmd = IpcCmd::ListStacks;
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, IpcCmd::ListStacks));

        // ClearStacks
        let cmd = IpcCmd::ClearStacks;
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, IpcCmd::ClearStacks));
    }

    #[test]
    fn test_ipc_stack_resp_serialization() {
        use crate::domain::stack::StackInfo;

        let resp = IpcStackResp {
            stacks: vec![
                StackInfo {
                    number: 1,
                    preview: "First stack...".to_string(),
                    created_at: "2024-01-01 00:00:00".to_string(),
                },
                StackInfo {
                    number: 2,
                    preview: "Second stack...".to_string(),
                    created_at: "2024-01-01 00:01:00".to_string(),
                },
            ],
            mode_enabled: true,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: IpcStackResp = serde_json::from_str(&json).unwrap();
        assert!(deserialized.mode_enabled);
        assert_eq!(deserialized.stacks.len(), 2);
        assert_eq!(deserialized.stacks[0].number, 1);
        assert_eq!(deserialized.stacks[1].number, 2);
    }

    #[test]
    fn test_backward_compatibility() {
        // 既存のIPCコマンドが引き続き動作することを確認
        let cmd = IpcCmd::Start {
            paste: true,
            prompt: None,
            direct_input: false,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("Start"));

        // 他の既存コマンドも確認
        let cmd = IpcCmd::Stop;
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, IpcCmd::Stop));

        let cmd = IpcCmd::Toggle {
            paste: false,
            prompt: Some("test".to_string()),
            direct_input: true,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();
        match deserialized {
            IpcCmd::Toggle {
                paste,
                prompt,
                direct_input,
            } => {
                assert!(!paste);
                assert_eq!(prompt, Some("test".to_string()));
                assert!(direct_input);
            }
            _ => panic!("Expected Toggle command"),
        }
    }
}
