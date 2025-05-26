pub mod memory_mode_test;
pub mod file_mode_test;
pub mod mode_switch_test;

use std::process::{Child, Command};
use std::time::Duration;
use std::thread;
use std::io::Write;

pub fn start_voice_inputd(use_file_mode: bool) -> Result<Child, std::io::Error> {
    let mut cmd = Command::new("target/debug/voice_inputd");
    
    if use_file_mode {
        cmd.env("LEGACY_TMP_WAV_FILE", "true");
    }
    
    cmd.spawn()
}

pub async fn voice_input_cli(args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("target/debug/voice_input")
        .args(args)
        .output()?;
    
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    
    if !output.status.success() {
        return Err(format!("Command failed with stderr: {}", stderr).into());
    }
    
    Ok(stdout + &stderr)
}

pub async fn simulate_audio_input(duration: Duration) -> Result<(), Box<dyn std::error::Error>> {
    // オーディオ入力のシミュレーション
    // 実際のテストでは、テスト用の音声データを生成するか、
    // モックを使用する必要があります
    thread::sleep(duration);
    Ok(())
}

pub fn wait_for_daemon_ready() -> Result<(), Box<dyn std::error::Error>> {
    // デーモンが起動するまで待機
    thread::sleep(Duration::from_secs(2));
    
    // デーモンの準備状態を確認
    for _ in 0..30 {
        match Command::new("target/debug/voice_input")
            .arg("status")
            .output()
        {
            Ok(output) if output.status.success() => {
                return Ok(());
            }
            _ => {
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    
    Err("Daemon failed to start within timeout".into())
}

pub fn kill_daemon(mut daemon: Child) -> Result<(), Box<dyn std::error::Error>> {
    daemon.kill()?;
    daemon.wait()?;
    Ok(())
}