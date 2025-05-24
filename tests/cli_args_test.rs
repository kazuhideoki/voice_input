use std::process::Command;

#[test]
fn test_direct_input_flag() {
    // --direct-inputフラグのテスト
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "voice_input",
            "--",
            "start",
            "--paste",
            "--direct-input",
        ])
        .output()
        .expect("Failed to run command");

    // デーモンが起動していない場合でも、引数パースは成功するはず
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
}

#[test]
fn test_no_direct_input_flag() {
    // --no-direct-inputフラグのテスト
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "voice_input",
            "--",
            "start",
            "--paste",
            "--no-direct-input",
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
            "--paste",
            "--direct-input",
            "--no-direct-input",
        ])
        .output()
        .expect("Failed to run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // エラーメッセージはstderrまたはstdoutに出力される可能性がある
    assert!(
        stderr.contains("Cannot specify both --direct-input and --no-direct-input")
            || stdout.contains("Cannot specify both --direct-input and --no-direct-input")
    );
}

#[test]
fn test_toggle_direct_input_flag() {
    // toggleコマンドでも--direct-inputが使えることを確認
    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "voice_input",
            "--",
            "toggle",
            "--paste",
            "--direct-input",
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
            "--paste",
            "--direct-input",
            "--no-direct-input",
        ])
        .output()
        .expect("Failed to run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // エラーメッセージはstderrまたはstdoutに出力される可能性がある
    assert!(
        stderr.contains("Cannot specify both --direct-input and --no-direct-input")
            || stdout.contains("Cannot specify both --direct-input and --no-direct-input")
    );
}

#[test]
fn test_help_shows_direct_input_flags() {
    // ヘルプに新しいフラグが表示されることを確認
    let output = Command::new("cargo")
        .args(&["run", "--bin", "voice_input", "--", "start", "--help"])
        .output()
        .expect("Failed to run command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--direct-input"));
    assert!(stdout.contains("--no-direct-input"));
    assert!(stdout.contains("Use direct text input instead of clipboard paste"));
    assert!(stdout.contains("Explicitly use clipboard paste"));
}

#[test]
fn test_default_behavior() {
    // フラグを指定しない場合のデフォルト動作（エラーにならないこと）
    let output = Command::new("cargo")
        .args(&["run", "--bin", "voice_input", "--", "start", "--paste"])
        .output()
        .expect("Failed to run command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // 引数パースエラーがないことを確認（デーモン接続エラーは無視）
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}
