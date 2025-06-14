//! 録音タイマーの動作を検証するテスト
//!
//! 30秒の自動停止タイマーが正しく動作し、
//! 予期しない早期停止が発生しないことを保証します。

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    use tokio::time::timeout;
    use voice_input::ipc::IpcCmd;

    /// テスト用の設定時間（短縮版）
    const TEST_AUTO_STOP_SECS: u64 = 3; // テストでは3秒に短縮

    // 注: 実際のテストはservice_container.rsのモジュール内で実装
}

// 代わりに、統合テストとして実装
#[tokio::test(flavor = "current_thread")]
#[ignore] // デーモンが必要なためignore
async fn test_recording_auto_stop_integration() {
    use std::process::Command;
    use std::time::{Duration, Instant};
    
    // デーモンが起動していることを前提
    
    // 録音開始
    let start_output = Command::new("./target/debug/voice_input")
        .arg("--no-paste")
        .output()
        .expect("Failed to start recording");
    
    assert!(start_output.status.success(), "Recording should start successfully");
    
    let start_time = Instant::now();
    
    // 録音状態を定期的にチェック
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let status_output = Command::new("./target/debug/voice_input")
            .arg("status")
            .output()
            .expect("Failed to get status");
        
        let status_str = String::from_utf8_lossy(&status_output.stdout);
        
        if status_str.contains("Idle") {
            let elapsed = start_time.elapsed();
            
            // 30秒（±1秒）で停止していることを確認
            assert!(
                elapsed >= Duration::from_secs(29) && elapsed <= Duration::from_secs(31),
                "Recording should stop at approximately 30 seconds, but stopped at {:?}",
                elapsed
            );
            break;
        }
        
        // タイムアウト（35秒）
        if start_time.elapsed() > Duration::from_secs(35) {
            panic!("Recording did not stop within expected time");
        }
    }
}