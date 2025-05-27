use std::time::Duration;
use std::process::Command;
use super::*;


#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_default_mode_is_memory() -> Result<(), Box<dyn std::error::Error>> {
    // デーモン起動（デフォルトモード）
    let daemon = start_voice_inputd()?;
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
