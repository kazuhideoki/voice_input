use std::process::Command;

fn run_cmd(args: &[&str]) -> std::process::Output {
    let mut command = Command::new("cargo");
    command
        .env("TRANSCRIPTION_PROVIDER", "4o")
        .env("TRANSCRIPTION_MODEL", "gpt-4o-transcribe")
        .args(["run", "--bin", "voice_input", "--"])
        .args(args);
    command.output().expect("Failed to run command")
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
fn help_hides_clipboard_flags() {
    let output = run_cmd(&["start", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("--copy-and-paste"));
    assert!(!stdout.contains("--copy-only"));
}

/// startコマンドがデフォルト引数で実行できる
#[test]
fn start_command_accepts_default_args() {
    let output = run_cmd(&["start"]);
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

/// toggleコマンドで音声保存パスを指定できる
#[test]
fn toggle_command_accepts_audio_save_path() {
    let output = run_cmd(&["toggle", "--save-audio", "/tmp/voice-input-debug.wav"]);
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

/// toggleコマンドで4o転写バックエンドを指定できる
#[test]
fn toggle_command_accepts_4o_transcription_provider() {
    let output = run_cmd(&["toggle", "--transcription-provider", "4o"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}

/// startコマンドは旧OpenAI別名を受け付けない
#[test]
fn start_command_rejects_openai_transcription_provider_alias() {
    let output = run_cmd(&["start", "--transcription-provider", "openai"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid value"));
}
