use std::process::Command;

fn run_cmd(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--bin", "voice_input", "--"])
        .args(args)
        .output()
        .expect("Failed to run command")
}

#[test]
fn test_copy_and_paste_flag_rejected() {
    let output = run_cmd(&["start", "--copy-and-paste"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("found argument"));
}

#[test]
fn test_copy_only_flag_rejected() {
    let output = run_cmd(&["start", "--copy-only"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("found argument"));
}

#[test]
fn test_help_does_not_show_clipboard_flags() {
    let output = run_cmd(&["start", "--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("--copy-and-paste"));
    assert!(!stdout.contains("--copy-only"));
}

#[test]
fn test_default_behavior() {
    let output = run_cmd(&["start"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("error: unexpected argument"));
    assert!(!stderr.contains("error: invalid value"));
}
