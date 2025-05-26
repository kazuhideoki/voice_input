use std::time::Duration;
use std::process::Command;
use super::*;

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_mode_switch_via_environment() -> Result<(), Box<dyn std::error::Error>> {
    // 最初はメモリモードで起動
    let daemon_memory = start_voice_inputd(false)?;
    wait_for_daemon_ready()?;
    
    // メモリモードで録音
    voice_input_cli(&["toggle"]).await?;
    simulate_audio_input(Duration::from_secs(2)).await?;
    voice_input_cli(&["toggle"]).await?;
    
    // メモリモードではファイルが作成されないことを確認
    let wav_files = count_wav_files();
    assert_eq!(wav_files, 0, "No WAV files in memory mode");
    
    // デーモンを終了
    kill_daemon(daemon_memory)?;
    thread::sleep(Duration::from_secs(1));
    
    // ファイルモードで再起動
    let daemon_file = start_voice_inputd(true)?;
    wait_for_daemon_ready()?;
    
    // ファイルモードで録音
    voice_input_cli(&["toggle"]).await?;
    simulate_audio_input(Duration::from_secs(2)).await?;
    voice_input_cli(&["toggle"]).await?;
    
    // ファイルモードではファイルが作成されることを確認
    let wav_files = count_wav_files();
    assert!(wav_files > 0, "WAV files should be created in file mode");
    
    // クリーンアップ
    cleanup_wav_files();
    kill_daemon(daemon_file)?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_default_mode_is_memory() -> Result<(), Box<dyn std::error::Error>> {
    // 環境変数を明示的に削除してデフォルト動作を確認
    std::env::remove_var("LEGACY_TMP_WAV_FILE");
    
    // デーモン起動（デフォルトモード）
    let daemon = start_voice_inputd(false)?;
    wait_for_daemon_ready()?;
    
    // 録音実行
    voice_input_cli(&["toggle"]).await?;
    simulate_audio_input(Duration::from_secs(2)).await?;
    voice_input_cli(&["toggle"]).await?;
    
    // デフォルトはメモリモード（ファイルが作成されない）
    let wav_files = count_wav_files();
    assert_eq!(wav_files, 0, "Default mode should be memory mode");
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_mode_persistence_across_recordings() -> Result<(), Box<dyn std::error::Error>> {
    // ファイルモードで起動
    let daemon = start_voice_inputd(true)?;
    wait_for_daemon_ready()?;
    
    // 複数回録音を実行
    for i in 1..=3 {
        println!("Recording {} in file mode", i);
        
        voice_input_cli(&["toggle"]).await?;
        simulate_audio_input(Duration::from_secs(1)).await?;
        voice_input_cli(&["toggle"]).await?;
        
        // 各録音でファイルが作成されることを確認
        let wav_files = count_wav_files();
        assert!(
            wav_files >= i,
            "WAV files should be created for each recording in file mode"
        );
        
        thread::sleep(Duration::from_millis(500));
    }
    
    // クリーンアップ
    cleanup_wav_files();
    kill_daemon(daemon)?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_invalid_environment_value() -> Result<(), Box<dyn std::error::Error>> {
    // 無効な環境変数値でもメモリモードにフォールバック
    let mut cmd = Command::new("target/debug/voice_inputd");
    cmd.env("LEGACY_TMP_WAV_FILE", "invalid_value");
    let daemon = cmd.spawn()?;
    
    wait_for_daemon_ready()?;
    
    // 録音実行
    voice_input_cli(&["toggle"]).await?;
    simulate_audio_input(Duration::from_secs(2)).await?;
    voice_input_cli(&["toggle"]).await?;
    
    // メモリモードで動作（ファイルが作成されない）
    let wav_files = count_wav_files();
    assert_eq!(
        wav_files, 0,
        "Invalid env value should fallback to memory mode"
    );
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}

// ヘルパー関数
fn count_wav_files() -> usize {
    std::fs::read_dir("/tmp")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().contains("voice_input_")
                && entry.file_name().to_string_lossy().ends_with(".wav")
        })
        .count()
}

fn cleanup_wav_files() {
    if let Ok(entries) = std::fs::read_dir("/tmp") {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.contains("voice_input_") && name_str.ends_with(".wav") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}