use std::process::Command;
use tempfile::TempDir;

fn run_cmd(args: &[&str]) -> std::process::Output {
    let mut command = Command::new("cargo");
    command
        .args(["run", "--bin", "voice_input", "--"])
        .args(args);
    command.output().expect("Failed to run command")
}

fn run_cmd_with_env(args: &[&str], key: &str, value: &str) -> std::process::Output {
    let mut command = Command::new("cargo");
    command
        .args(["run", "--bin", "voice_input", "--"])
        .args(args)
        .env(key, value);
    command.output().expect("Failed to run command")
}

fn run_built_cmd_with_runtime_dir(args: &[&str], runtime_dir: &TempDir) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_voice_input"))
        .args(args)
        .env("XDG_DATA_HOME", runtime_dir.path())
        .env(
            "VOICE_INPUT_SOCKET_PATH",
            runtime_dir.path().join("missing.sock"),
        )
        .env(
            "VOICE_INPUT_DEFAULT_TRANSCRIPTION_PROVIDER",
            "gpt-transcribe",
        )
        .output()
        .expect("Failed to run built command")
}

/// 廃止されたcopy-and-pasteフラグは拒否される
#[test]
fn copy_and_paste_flag_is_rejected() {
    let output = run_cmd(&["start", "--copy-and-paste"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("found argument"));
}

/// 廃止されたcopy-onlyフラグは拒否される
#[test]
fn copy_only_flag_is_rejected() {
    let output = run_cmd(&["start", "--copy-only"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("found argument"));
}

/// ヘルプに廃止フラグが表示されない
#[test]
fn help_hides_removed_flags() {
    let output = run_cmd(&["start", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("--copy-and-paste"));
    assert!(!stdout.contains("--copy-only"));
    assert!(!stdout.contains("--prompt"));
    assert!(!stdout.contains("--transcription-model"));
}

/// startコマンドがデフォルト引数で実行できる
#[test]
fn start_command_accepts_default_args() {
    let output = run_cmd(&["start"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// historyコマンドが実行できる
#[test]
fn history_command_accepts_default_args() {
    let output = run_cmd(&["history"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// startコマンドで音声保存パスを指定できる
#[test]
fn start_command_accepts_audio_save_path() {
    let output = run_cmd(&["start", "--save-audio", "/tmp/voice-input-debug.wav"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// startコマンドで最大録音秒数を指定できる
#[test]
fn start_command_accepts_max_secs() {
    let output = run_cmd(&["start", "--max-secs", "120"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// startコマンドは0秒の最大録音秒数を拒否する
#[test]
fn start_command_rejects_zero_max_secs() {
    let output = run_cmd(&["start", "--max-secs", "0"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value"));
}

/// startコマンドは負数の最大録音秒数を拒否する
#[test]
fn start_command_rejects_negative_max_secs() {
    let output = run_cmd(&["start", "--max-secs=-1"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value"));
}

/// startコマンドの最大録音秒数は不正な環境変数より優先される
#[test]
fn start_command_max_secs_overrides_invalid_env() {
    let output = run_cmd_with_env(
        &["start", "--max-secs", "120"],
        "VOICE_INPUT_DEFAULT_MAX_SECS",
        "abc",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("VOICE_INPUT_DEFAULT_MAX_SECS must be a positive integer"));
}

/// startコマンドで入力ファイルパスを指定できる
#[test]
fn start_command_accepts_input_file_path() {
    let output = run_cmd(&["start", "--input-file", "/tmp/voice-input-debug.wav"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// toggleコマンドで音声保存パスを指定できる
#[test]
fn toggle_command_accepts_audio_save_path() {
    let output = run_cmd(&["toggle", "--save-audio", "/tmp/voice-input-debug.wav"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// toggleコマンドで最大録音秒数を指定できる
#[test]
fn toggle_command_accepts_max_secs() {
    let output = run_cmd(&["toggle", "--max-secs", "90"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// toggleコマンドで入力ファイルパスを指定できる
#[test]
fn toggle_command_accepts_input_file_path() {
    let output = run_cmd(&["toggle", "--input-file", "/tmp/voice-input-debug.wav"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// startコマンドでMLX転写バックエンドを指定できる
#[test]
fn start_command_accepts_mlx_transcription_provider() {
    let output = run_cmd(&["start", "--transcription-provider", "mlx-qwen3-asr"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// startコマンドでGPT Live Transcribe転写バックエンドを指定できる
#[test]
fn start_command_accepts_gpt_live_transcribe_provider() {
    let output = run_cmd(&["start", "--transcription-provider", "gpt-live-transcribe"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// startコマンドは廃止したRealtime Whisper転写バックエンドを拒否する
#[test]
fn start_command_rejects_removed_realtime_whisper_provider() {
    let output = run_cmd(&["start", "--transcription-provider", "realtime-whisper"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value"));
}

/// toggleコマンドでGPT Transcribe転写バックエンドを指定できる
#[test]
fn toggle_command_accepts_gpt_transcribe_provider() {
    let output = run_cmd(&["toggle", "--transcription-provider", "gpt-transcribe"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// startコマンドは廃止した4o転写バックエンドを拒否する
#[test]
fn start_command_rejects_removed_4o_provider() {
    let output = run_cmd(&["start", "--transcription-provider", "4o"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value"));
}

/// startコマンドは廃止した転写モデル指定を拒否する
#[test]
fn start_command_rejects_removed_transcription_model_flag() {
    let output = run_cmd(&["start", "--transcription-model", "gpt-transcribe"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("found argument"));
}

/// startコマンドは廃止したプロンプト指定を拒否する
#[test]
fn start_command_rejects_removed_prompt_flag() {
    let output = run_cmd(&["start", "--prompt", "context"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("found argument"));
}

/// startコマンドは旧OpenAI別名を受け付けない
#[test]
fn start_command_rejects_openai_transcription_provider_alias() {
    let output = run_cmd(&["start", "--transcription-provider", "openai"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value"));
}

/// ユーザーが変更可能な実行時設定を永続化し個別に解除できる
#[test]
fn runtime_configuration_persists_and_unsets_supported_fields() {
    let runtime_dir = TempDir::new().unwrap();

    for args in [
        vec![
            "config",
            "set",
            "transcription-provider",
            "gpt-live-transcribe",
        ],
        vec!["config", "set", "max-secs", "90"],
        vec!["config", "set", "pre-roll-ms", "250"],
        vec![
            "config",
            "set",
            "input-device-priorities",
            "External Mic",
            "MacBook Microphone",
        ],
        vec!["config", "set", "recording-sounds-enabled", "false"],
        vec!["config", "set", "recording-hud-enabled", "false"],
        vec!["config", "set", "push-to-talk-enabled", "true"],
        vec!["config", "set", "push-to-talk-hotkey", "cmd+space"],
        vec!["config", "set", "transcribe-streaming", "true"],
    ] {
        let output = run_built_cmd_with_runtime_dir(&args, &runtime_dir);
        assert!(output.status.success());
    }

    let config_path = runtime_dir.path().join("voice_input/config.json");
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(config_path).unwrap()).unwrap();
    assert_eq!(config["transcription_provider"], "gpt-live-transcribe");
    assert_eq!(config["max_secs"], 90);
    assert_eq!(config["pre_roll_ms"], 250);
    assert_eq!(config["input_device_priorities"][0], "External Mic");
    assert_eq!(config["input_device_priorities"][1], "MacBook Microphone");
    assert_eq!(config["recording_sounds_enabled"], false);
    assert_eq!(config["recording_hud_enabled"], false);
    assert_eq!(config["push_to_talk_enabled"], true);
    assert_eq!(config["push_to_talk_hotkey"], "cmd+space");
    assert_eq!(config["transcribe_streaming"], true);

    let output =
        run_built_cmd_with_runtime_dir(&["config", "get", "transcription-provider"], &runtime_dir);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "gpt-live-transcribe"
    );

    let output = run_built_cmd_with_runtime_dir(&["config", "show"], &runtime_dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dict-path="));
    assert!(stdout.contains("transcription-provider=gpt-live-transcribe"));
    assert!(stdout.contains("max-secs=90"));
    assert!(stdout.contains("pre-roll-ms=250"));
    assert!(stdout.contains("input-device-priorities=External Mic,MacBook Microphone"));
    assert!(stdout.contains("recording-sounds-enabled=false"));
    assert!(stdout.contains("recording-hud-enabled=false"));
    assert!(stdout.contains("push-to-talk-enabled=true"));
    assert!(stdout.contains("push-to-talk-hotkey=cmd+space"));
    assert!(stdout.contains("transcribe-streaming=true"));

    for field in [
        "transcription-provider",
        "max-secs",
        "pre-roll-ms",
        "input-device-priorities",
        "recording-sounds-enabled",
        "recording-hud-enabled",
        "push-to-talk-enabled",
        "push-to-talk-hotkey",
        "transcribe-streaming",
    ] {
        let output = run_built_cmd_with_runtime_dir(&["config", "unset", field], &runtime_dir);
        assert!(output.status.success());
    }
}

/// 永続設定で上書きした項目は不正な環境既定値に影響されない
#[test]
fn persisted_configuration_shadows_invalid_environment_default() {
    let runtime_dir = TempDir::new().unwrap();
    let config_dir = runtime_dir.path().join("voice_input");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.json"), r#"{"max_secs":90}"#).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_voice_input"))
        .args(["config", "get", "max-secs"])
        .env("XDG_DATA_HOME", runtime_dir.path())
        .env(
            "VOICE_INPUT_SOCKET_PATH",
            runtime_dir.path().join("missing.sock"),
        )
        .env("VOICE_INPUT_DEFAULT_MAX_SECS", "invalid")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "90");
}
