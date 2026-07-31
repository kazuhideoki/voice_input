use std::path::PathBuf;

use voice_input::ipc::IpcCmd;
use voice_input::utils::config::TranscriptionProvider;

/// 各種IpcCmdがJSONラウンドトリップで同一になる
#[test]
fn ipc_commands_roundtrip_via_json() {
    let commands = vec![
        IpcCmd::Start {
            save_audio_path: None,
            max_duration_secs: None,
            transcription_provider: None,
        },
        IpcCmd::Toggle {
            save_audio_path: Some(PathBuf::from("/tmp/debug.flac")),
            max_duration_secs: Some(120),
            transcription_provider: Some(TranscriptionProvider::GptTranscribe),
        },
        IpcCmd::StartWithInputFile {
            save_audio_path: None,
            max_duration_secs: None,
            input_file_path: PathBuf::from("/tmp/sample.wav"),
            transcription_provider: Some(TranscriptionProvider::MlxQwen3Asr),
        },
        IpcCmd::ToggleWithInputFile {
            save_audio_path: None,
            max_duration_secs: None,
            input_file_path: PathBuf::from("/tmp/sample.wav"),
            transcription_provider: Some(TranscriptionProvider::GptLiveTranscribe),
        },
        IpcCmd::Stop,
        IpcCmd::Status,
        IpcCmd::History,
        IpcCmd::Health,
        IpcCmd::ListDevices,
    ];

    for command in commands {
        let json = serde_json::to_string(&command).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized, command);
    }
}

/// Startコマンドが音声保存パスをJSONで保持する
#[test]
fn start_command_preserves_audio_save_path() {
    let command = IpcCmd::Start {
        save_audio_path: Some(PathBuf::from("/tmp/debug.wav")),
        max_duration_secs: None,
        transcription_provider: None,
    };

    let json = serde_json::to_string(&command).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized, command);
    assert!(json.contains(r#""save_audio_path":"/tmp/debug.wav""#));
}

/// Startコマンドが最大録音秒数をJSONで保持する
#[test]
fn start_command_preserves_max_duration_secs() {
    let command = IpcCmd::Start {
        save_audio_path: None,
        max_duration_secs: Some(120),
        transcription_provider: None,
    };

    let json = serde_json::to_string(&command).unwrap();

    assert!(json.contains(r#""max_duration_secs":120"#));
    assert_eq!(serde_json::from_str::<IpcCmd>(&json).unwrap(), command);
}

/// ファイル入力コマンドが入力ファイルパスをJSONで保持する
#[test]
fn input_file_commands_preserve_input_path() {
    let command = IpcCmd::StartWithInputFile {
        save_audio_path: None,
        max_duration_secs: None,
        input_file_path: PathBuf::from("/tmp/sample.wav"),
        transcription_provider: None,
    };

    let json = serde_json::to_string(&command).unwrap();

    assert!(json.contains(r#""input_file_path":"/tmp/sample.wav""#));
    assert_eq!(serde_json::from_str::<IpcCmd>(&json).unwrap(), command);
}

/// GPT Transcribeプロバイダ指定をJSON文字列で保持する
#[test]
fn start_command_preserves_gpt_transcribe_provider() {
    let command = IpcCmd::Start {
        save_audio_path: None,
        max_duration_secs: None,
        transcription_provider: Some(TranscriptionProvider::GptTranscribe),
    };

    let json = serde_json::to_string(&command).unwrap();

    assert!(json.contains(r#""transcription_provider":"gpt-transcribe""#));
    assert_eq!(serde_json::from_str::<IpcCmd>(&json).unwrap(), command);
}

/// GPT Live Transcribeプロバイダ指定をJSON文字列で保持する
#[test]
fn start_command_preserves_gpt_live_transcribe_provider() {
    let command = IpcCmd::Start {
        save_audio_path: None,
        max_duration_secs: None,
        transcription_provider: Some(TranscriptionProvider::GptLiveTranscribe),
    };

    let json = serde_json::to_string(&command).unwrap();

    assert!(json.contains(r#""transcription_provider":"gpt-live-transcribe""#));
    assert_eq!(serde_json::from_str::<IpcCmd>(&json).unwrap(), command);
}

/// 廃止したRealtime Whisperプロバイダを含むIPCは拒否する
#[test]
fn ipc_rejects_removed_realtime_whisper_provider() {
    let json = r#"{"Start":{"save_audio_path":null,"max_duration_secs":null,"transcription_provider":"realtime-whisper"}}"#;

    assert!(serde_json::from_str::<IpcCmd>(json).is_err());
}
