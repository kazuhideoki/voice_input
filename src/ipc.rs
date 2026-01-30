//! Unix Domain Socket (UDS) ベースのシンプルな IPC モジュール。
//! `voice_input` CLI ↔ `voice_inputd` デーモン間の通信で利用します。
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
        #[serde(default)]
        prompt: Option<String>,
    },
    /// 録音停止
    Stop,
    /// 録音トグル
    Toggle {
        #[serde(default)]
        prompt: Option<String>,
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

/// シリアライズ可能な音声データ（メモリモード専用）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AudioDataDto(pub Vec<u8>);

/// 録音結果を表す構造体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingResult {
    pub audio_data: AudioDataDto,
    pub duration_ms: u64,
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
        AudioData {
            bytes: dto.0,
            mime_type: mime.0,
            file_name: mime.1.to_string(),
        }
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

    /// 環境変数が未設定ならデフォルトのソケットパスを使う
    #[test]
    fn socket_path_uses_default_when_env_unset() {
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

    /// ソケットパス環境変数が設定されていれば優先される
    #[test]
    fn socket_path_uses_env_override() {
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

    /// ソケットディレクトリ環境変数が設定されていれば反映される
    #[test]
    fn socket_path_uses_env_dir_override() {
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

    /// AudioDataDtoがバイト列を保持する
    #[test]
    fn audio_data_dto_holds_bytes() {
        let wav_data = vec![0u8, 1, 2, 3, 4, 5];
        let audio_data = AudioDataDto(wav_data.clone());

        assert_eq!(audio_data.0, wav_data);
    }

    /// AudioDataDtoがJSONで往復できる
    #[test]
    fn audio_data_dto_roundtrips_json() {
        let wav_data = vec![0u8, 1, 2, 3, 4, 5];
        let audio_data = AudioDataDto(wav_data.clone());

        let json = serde_json::to_string(&audio_data).unwrap();
        let deserialized: AudioDataDto = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.0, wav_data);
    }

    /// RecordingResultが音声と時間を保持する
    #[test]
    fn recording_result_holds_audio_and_duration() {
        let audio_data = AudioDataDto(vec![1, 2, 3]);
        let duration_ms = 1500u64;

        let result = RecordingResult {
            audio_data: audio_data.clone(),
            duration_ms,
        };

        assert_eq!(result.duration_ms, 1500);
        assert_eq!(result.audio_data.0, vec![1, 2, 3]);
    }

    /// RecordingResultがフィールドを保持できる
    #[test]
    fn recording_result_stores_fields() {
        let audio_data = AudioDataDto(vec![10, 20, 30]);
        let duration_ms = 3000u64;

        let result = RecordingResult {
            audio_data,
            duration_ms,
        };

        assert_eq!(result.duration_ms, 3000);
        assert_eq!(result.audio_data.0, vec![10, 20, 30]);
    }

    /// AudioDataDtoがJSONでシリアライズできる
    #[test]
    fn audio_data_dto_serializes_to_json() {
        let data = AudioDataDto(vec![1, 2, 3, 4, 5]);
        let json = serde_json::to_string(&data).unwrap();
        let deserialized: AudioDataDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.0, vec![1, 2, 3, 4, 5]);
    }

    /// RecordingResultがJSONで往復できる
    #[test]
    fn recording_result_roundtrips_json() {
        let result = RecordingResult {
            audio_data: AudioDataDto(vec![10, 20, 30]),
            duration_ms: 2500,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: RecordingResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.duration_ms, 2500);
        assert_eq!(deserialized.audio_data.0, vec![10, 20, 30]);
    }

    /// AudioDataからAudioDataDtoへ変換できる
    #[test]
    fn audio_data_converts_to_dto() {
        let audio_data = AudioData {
            bytes: vec![1, 2, 3, 4],
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };
        let dto: AudioDataDto = audio_data.into();
        assert_eq!(dto.0, vec![1, 2, 3, 4]);
    }

    /// AudioDataDtoからAudioDataへ変換できる
    #[test]
    fn dto_converts_to_audio_data() {
        let dto = AudioDataDto(vec![5, 6, 7, 8]);
        let audio_data: AudioData = dto.into();
        assert_eq!(audio_data.bytes, vec![5, 6, 7, 8]);
    }

    /// IpcCmd/IpcRespがJSONで互換性を保つ
    #[test]
    fn ipc_cmd_and_resp_roundtrip() {
        // Test that existing IPC commands still work
        let cmd = IpcCmd::Start {
            prompt: Some("test prompt".to_string()),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

        match deserialized {
            IpcCmd::Start { prompt } => {
                assert_eq!(prompt, Some("test prompt".to_string()));
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

    /// 既存IPCコマンドが後方互換で動作する
    #[test]
    fn ipc_commands_remain_backward_compatible() {
        // 既存のIPCコマンドが引き続き動作することを確認
        let cmd = IpcCmd::Start { prompt: None };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("Start"));

        // 他の既存コマンドも確認
        let cmd = IpcCmd::Stop;
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, IpcCmd::Stop));

        let cmd = IpcCmd::Toggle {
            prompt: Some("test".to_string()),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();
        match deserialized {
            IpcCmd::Toggle { prompt } => {
                assert_eq!(prompt, Some("test".to_string()));
            }
            _ => panic!("Expected Toggle command"),
        }
    }
}
