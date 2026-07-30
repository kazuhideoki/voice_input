use std::process::Command;

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
        "VOICE_INPUT_MAX_SECS",
        "abc",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("VOICE_INPUT_MAX_SECS must be a positive integer"));
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

/// startコマンドでRealtime Whisper転写バックエンドを指定できる
#[test]
fn start_command_accepts_realtime_whisper_transcription_provider() {
    let output = run_cmd(&["start", "--transcription-provider", "realtime-whisper"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
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
