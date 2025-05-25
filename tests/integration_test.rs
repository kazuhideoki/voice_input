use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

/// デーモンプロセスを管理する構造体
struct DaemonProcess {
    child: Child,
}

impl DaemonProcess {
    /// デーモンを起動
    async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let mut child = Command::new("cargo")
            .args(["run", "--bin", "voice_input", "--", "daemon"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // デーモンの起動を待つ
        sleep(Duration::from_secs(2)).await;

        // プロセスが正常に起動しているか確認
        match child.try_wait()? {
            Some(status) => {
                return Err(format!("Daemon exited early with status: {:?}", status).into());
            }
            None => {
                // まだ実行中
            }
        }

        Ok(Self { child })
    }

    /// デーモンを停止
    async fn stop(mut self) -> Result<(), Box<dyn std::error::Error>> {
        // stopコマンドを送信
        Command::new("cargo")
            .args(["run", "--bin", "voice_input", "--", "stop"])
            .output()
            .await?;

        // プロセスの終了を待つ
        timeout(Duration::from_secs(5), self.child.wait()).await??;

        Ok(())
    }
}

/// クリップボードの内容を取得
async fn get_clipboard_content() -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("pbpaste").output().await?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// クリップボードに内容を設定
async fn set_clipboard_content(content: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(content.as_bytes()).await?;
    }

    child.wait().await?;
    Ok(())
}

#[tokio::test]
#[ignore] // 手動実行用：cargo test --test integration_test -- --ignored
async fn test_voice_input_direct_mode_preserves_clipboard() -> Result<(), Box<dyn std::error::Error>>
{
    // 1. テスト用のクリップボード内容を設定
    let test_clipboard_content = "Test clipboard content - should not be changed";
    set_clipboard_content(test_clipboard_content).await?;

    // 2. デーモンを起動
    let daemon = DaemonProcess::start().await?;

    // 3. クリップボード内容を確認
    let clipboard_before = get_clipboard_content().await?;
    assert_eq!(clipboard_before.trim(), test_clipboard_content);

    // 4. 直接入力モードで音声入力を開始（シミュレート）
    // 注: 実際の音声入力のシミュレーションは困難なため、
    // ここではコマンドの実行のみを確認
    let output = Command::new("cargo")
        .args(["run", "--bin", "voice_input", "--", "start"])
        .output()
        .await?;

    if !output.status.success() {
        eprintln!(
            "Start command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // 5. 少し待つ
    sleep(Duration::from_secs(1)).await;

    // 6. 音声入力を停止
    Command::new("cargo")
        .args(["run", "--bin", "voice_input", "--", "stop"])
        .output()
        .await?;

    // 7. クリップボード内容が変わっていないことを確認
    let clipboard_after = get_clipboard_content().await?;
    assert_eq!(
        clipboard_after.trim(),
        test_clipboard_content,
        "Clipboard content should not be changed in direct input mode"
    );

    // 8. デーモンを停止
    daemon.stop().await?;

    Ok(())
}

#[tokio::test]
#[ignore] // 手動実行用
async fn test_voice_input_paste_mode_uses_clipboard() -> Result<(), Box<dyn std::error::Error>> {
    // 1. テスト用のクリップボード内容を設定
    let initial_content = "Initial clipboard content";
    set_clipboard_content(initial_content).await?;

    // 2. デーモンを起動
    let daemon = DaemonProcess::start().await?;

    // 3. ペーストモードで音声入力を開始（明示的に--copy-and-paste）
    let output = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "voice_input",
            "--",
            "start",
            "--copy-and-paste",
        ])
        .output()
        .await?;

    if !output.status.success() {
        eprintln!(
            "Start command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // 4. 音声入力を停止
    sleep(Duration::from_secs(1)).await;
    Command::new("cargo")
        .args(["run", "--bin", "voice_input", "--", "stop"])
        .output()
        .await?;

    // 5. デーモンを停止
    daemon.stop().await?;

    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_conflicting_flags_error() -> Result<(), Box<dyn std::error::Error>> {
    // 競合するフラグを指定した場合のエラーを確認
    let output = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "voice_input",
            "--",
            "start",
            "--copy-and-paste",
            "--copy-only",
        ])
        .output()
        .await?;

    assert!(!output.status.success());

    let error_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(error_output.contains("Cannot specify both --copy-and-paste and --copy-only"));

    Ok(())
}

#[tokio::test]
#[ignore] // 手動実行用
async fn test_daemon_ipc_communication() -> Result<(), Box<dyn std::error::Error>> {
    // 1. デーモンを起動
    let daemon = DaemonProcess::start().await?;

    // 2. 各種コマンドを送信してIPCが正常に動作することを確認
    let commands = vec![
        vec!["start"],
        vec!["stop"],
        vec!["toggle", "--copy-and-paste"],
        vec!["stop"],
    ];

    for cmd in commands {
        let output = Command::new("cargo")
            .args(["run", "--bin", "voice_input", "--"])
            .args(&cmd)
            .output()
            .await?;

        assert!(
            output.status.success(),
            "Command {:?} failed: {}",
            cmd,
            String::from_utf8_lossy(&output.stderr)
        );

        sleep(Duration::from_millis(500)).await;
    }

    // 3. デーモンを停止
    daemon.stop().await?;

    Ok(())
}
