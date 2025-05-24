use std::process::Command;

#[test]
fn test_copy_and_paste_flag() {
    // --copy-and-pasteフラグのテスト
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "voice_input",
            "--",
            "start",
            "--copy-and-paste",
        ])
        .output()
        .expect("Failed to run command");

    // デーモンが起動していない場合でも、引数パースは成功するはず
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
}

#[test]
fn test_copy_only_flag() {
    // --copy-onlyフラグのテスト
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "voice_input",
            "--",
            "start",
            "--copy-only",
        ])
        .output()
        .expect("Failed to run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
}

#[test]
fn test_conflicting_flags() {
    // 両方のフラグを指定した場合のエラーテスト
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "voice_input",
            "--",
            "start",
            "--copy-and-paste",
            "--copy-only",
        ])
        .output()
        .expect("Failed to run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // エラーメッセージはstderrまたはstdoutに出力される可能性がある
    assert!(
        stderr.contains("Cannot specify both --copy-and-paste and --copy-only")
            || stdout.contains("Cannot specify both --copy-and-paste and --copy-only")
    );
}

#[test]
fn test_toggle_copy_and_paste_flag() {
    // toggleコマンドでも--copy-and-pasteが使えることを確認
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "voice_input",
            "--",
            "toggle",
            "--copy-and-paste",
        ])
        .output()
        .expect("Failed to run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
}

#[test]
fn test_toggle_conflicting_flags() {
    // toggleコマンドでもフラグ競合エラーが発生することを確認
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "voice_input",
            "--",
            "toggle",
            "--copy-and-paste",
            "--copy-only",
        ])
        .output()
        .expect("Failed to run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // エラーメッセージはstderrまたはstdoutに出力される可能性がある
    assert!(
        stderr.contains("Cannot specify both --copy-and-paste and --copy-only")
            || stdout.contains("Cannot specify both --copy-and-paste and --copy-only")
    );
}

#[test]
fn test_help_shows_new_flags() {
    // ヘルプに新しいフラグが表示されることを確認
    let output = Command::new("cargo")
        .args(&["run", "--bin", "voice_input", "--", "start", "--help"])
        .output()
        .expect("Failed to run command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--copy-and-paste"));
    assert!(stdout.contains("--copy-only"));
    assert!(stdout.contains("Use clipboard copy-and-paste method"));
    assert!(stdout.contains("Only copy to clipboard without pasting"));
}

#[test]
fn test_default_behavior() {
    // フラグを指定しない場合のデフォルト動作（直接入力）
    let output = Command::new("cargo")
        .args(&["run", "--bin", "voice_input", "--", "start"])
        .output()
        .expect("Failed to run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // 引数パースエラーがないことを確認（デーモン接続エラーは無視）
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}
