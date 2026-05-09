use std::path::PathBuf;
use voice_input::ipc::IpcCmd;
use voice_input::utils::config::TranscriptionProvider;

/// Startコマンドがシリアライズ/デシリアライズで保持される
#[test]
fn start_command_serializes_roundtrip() {
    let start_cmd = IpcCmd::Start {
        prompt: Some("test prompt".to_string()),
        save_audio_path: None,
        max_duration_secs: None,
        transcription_provider: None,
        transcription_model: None,
    };

    let json = serde_json::to_string(&start_cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Start { prompt, .. } => {
            assert_eq!(prompt, Some("test prompt".to_string()));
        }
        _ => panic!("Expected Start command"),
    }
}

/// Toggleコマンドがシリアライズ/デシリアライズで保持される
#[test]
fn toggle_command_serializes_roundtrip() {
    let toggle_cmd = IpcCmd::Toggle {
        prompt: None,
        save_audio_path: None,
        max_duration_secs: None,
        transcription_provider: None,
        transcription_model: None,
    };

    let json = serde_json::to_string(&toggle_cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Toggle { prompt, .. } => {
            assert_eq!(prompt, None);
        }
        _ => panic!("Expected Toggle command"),
    }
}

/// 各種IpcCmdがJSONラウンドトリップで同一になる
#[test]
fn ipc_cmds_roundtrip_via_json() {
    // Test various combinations
    let commands = vec![
        IpcCmd::Start {
            prompt: None,
            save_audio_path: None,
            max_duration_secs: None,
            transcription_provider: None,
            transcription_model: None,
        },
        IpcCmd::Start {
            prompt: Some("hello".to_string()),
            save_audio_path: None,
            max_duration_secs: None,
            transcription_provider: None,
            transcription_model: None,
        },
        IpcCmd::Toggle {
            prompt: Some("world".to_string()),
            save_audio_path: None,
            max_duration_secs: None,
            transcription_provider: None,
            transcription_model: None,
        },
        IpcCmd::StartWithInputFile {
            prompt: Some("file".to_string()),
            save_audio_path: None,
            max_duration_secs: None,
            input_file_path: PathBuf::from("/tmp/sample.wav"),
            transcription_provider: None,
            transcription_model: None,
        },
        IpcCmd::ToggleWithInputFile {
            prompt: Some("file".to_string()),
            save_audio_path: None,
            max_duration_secs: None,
            input_file_path: PathBuf::from("/tmp/sample.wav"),
            transcription_provider: None,
            transcription_model: None,
        },
        IpcCmd::Stop,
        IpcCmd::Status,
        IpcCmd::History,
        IpcCmd::Health,
        IpcCmd::ListDevices,
    ];

    for cmd in commands {
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

        // Verify the JSON round-trip preserves the data
        let json2 = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json, json2);
    }
}

/// StartコマンドのJSONに必要なフィールドが含まれる
#[test]
fn start_command_json_format_contains_prompt() {
    // Verify the actual JSON format
    let cmd = IpcCmd::Start {
        prompt: Some("test".to_string()),
        save_audio_path: None,
        max_duration_secs: None,
        transcription_provider: None,
        transcription_model: None,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("\"Start\""));
    assert!(json.contains("\"prompt\":\"test\""));
}

/// Startコマンドが音声保存パスをJSONで保持する
#[test]
fn start_command_preserves_audio_save_path() {
    let cmd = IpcCmd::Start {
        prompt: None,
        save_audio_path: Some(PathBuf::from("/tmp/debug.wav")),
        max_duration_secs: None,
        transcription_provider: None,
        transcription_model: None,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Start {
            save_audio_path, ..
        } => {
            assert_eq!(save_audio_path, Some(PathBuf::from("/tmp/debug.wav")));
        }
        _ => panic!("Expected Start command"),
    }
}

/// Startコマンドが最大録音秒数をJSONで保持する
#[test]
fn start_command_preserves_max_duration_secs() {
    let cmd = IpcCmd::Start {
        prompt: None,
        save_audio_path: None,
        max_duration_secs: Some(120),
        transcription_provider: None,
        transcription_model: None,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains(r#""max_duration_secs":120"#));

    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Start {
            max_duration_secs, ..
        } => {
            assert_eq!(max_duration_secs, Some(120));
        }
        _ => panic!("Expected Start command"),
    }
}

/// Startコマンドが入力ファイルパスをJSONで保持する
#[test]
fn start_command_preserves_input_file_path() {
    let cmd = IpcCmd::StartWithInputFile {
        prompt: None,
        save_audio_path: None,
        max_duration_secs: None,
        input_file_path: PathBuf::from("/tmp/sample.wav"),
        transcription_provider: None,
        transcription_model: None,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::StartWithInputFile {
            input_file_path, ..
        } => {
            assert_eq!(input_file_path, PathBuf::from("/tmp/sample.wav"));
        }
        _ => panic!("Expected StartWithInputFile command"),
    }
}

/// ToggleWithInputFileコマンドが入力ファイルパスをJSONで保持する
#[test]
fn toggle_with_input_file_command_preserves_input_file_path() {
    let cmd = IpcCmd::ToggleWithInputFile {
        prompt: None,
        save_audio_path: None,
        max_duration_secs: None,
        input_file_path: PathBuf::from("/tmp/sample.wav"),
        transcription_provider: None,
        transcription_model: None,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::ToggleWithInputFile {
            input_file_path, ..
        } => {
            assert_eq!(input_file_path, PathBuf::from("/tmp/sample.wav"));
        }
        _ => panic!("Expected ToggleWithInputFile command"),
    }
}

/// Startコマンドが転写プロバイダ指定をJSONで保持する
#[test]
fn start_command_preserves_transcription_provider() {
    let cmd = IpcCmd::Start {
        prompt: None,
        save_audio_path: None,
        max_duration_secs: None,
        transcription_provider: Some(TranscriptionProvider::MlxQwen3Asr),
        transcription_model: None,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Start {
            transcription_provider,
            ..
        } => {
            assert_eq!(
                transcription_provider,
                Some(TranscriptionProvider::MlxQwen3Asr)
            );
        }
        _ => panic!("Expected Start command"),
    }
}

/// StartコマンドがRealtime Whisper指定をJSON文字列で保持する
#[test]
fn start_command_preserves_realtime_whisper_transcription_provider() {
    let cmd = IpcCmd::Start {
        prompt: None,
        save_audio_path: None,
        max_duration_secs: None,
        transcription_provider: Some(TranscriptionProvider::OpenAiRealtimeWhisper),
        transcription_model: None,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains(r#""transcription_provider":"realtime-whisper""#));

    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Start {
            transcription_provider,
            ..
        } => {
            assert_eq!(
                transcription_provider,
                Some(TranscriptionProvider::OpenAiRealtimeWhisper)
            );
        }
        _ => panic!("Expected Start command"),
    }
}

/// Startコマンドが転写モデル指定をJSONで保持する
#[test]
fn start_command_preserves_transcription_model() {
    let cmd = IpcCmd::Start {
        prompt: None,
        save_audio_path: None,
        max_duration_secs: None,
        transcription_provider: Some(TranscriptionProvider::OpenAi4o),
        transcription_model: Some("gpt-4o-mini-transcribe".to_string()),
    };

    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains(r#""transcription_model":"gpt-4o-mini-transcribe""#));

    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Start {
            transcription_model,
            ..
        } => {
            assert_eq!(
                transcription_model,
                Some("gpt-4o-mini-transcribe".to_string())
            );
        }
        _ => panic!("Expected Start command"),
    }
}
