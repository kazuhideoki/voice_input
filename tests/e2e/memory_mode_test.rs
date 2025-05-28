use std::time::Duration;
use super::*;

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_memory_mode_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    // デーモン起動（メモリモード）
    let daemon = start_voice_inputd()?;
    wait_for_daemon_ready()?;
    
    // 録音開始
    let result = voice_input_cli(&["toggle"]).await?;
    assert!(
        result.contains("Recording started") || result.contains("録音を開始"),
        "Expected recording start message, got: {}",
        result
    );
    
    // 音声シミュレーション（3秒）
    simulate_audio_input(Duration::from_secs(3)).await?;
    
    // 録音停止と転写
    let result = voice_input_cli(&["toggle"]).await?;
    assert!(
        result.contains("Transcription:") || result.contains("転写結果:"),
        "Expected transcription result, got: {}",
        result
    );
    
    // メモリモードの確認（ファイルが作成されていないこと）
    let wav_files = std::fs::read_dir("/tmp")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().contains("voice_input_")
                && entry.file_name().to_string_lossy().ends_with(".wav")
        })
        .count();
    
    assert_eq!(wav_files, 0, "No WAV files should be created in memory mode");
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_memory_mode_long_recording() -> Result<(), Box<dyn std::error::Error>> {
    // デーモン起動（メモリモード）
    let daemon = start_voice_inputd()?;
    wait_for_daemon_ready()?;
    
    // 録音開始
    let result = voice_input_cli(&["toggle"]).await?;
    assert!(
        result.contains("Recording started") || result.contains("録音を開始"),
        "Expected recording start message"
    );
    
    // 長時間録音シミュレーション（30秒）
    simulate_audio_input(Duration::from_secs(30)).await?;
    
    // 録音停止と転写
    let result = voice_input_cli(&["toggle"]).await?;
    assert!(
        result.contains("Transcription:") || result.contains("転写結果:"),
        "Expected transcription result after long recording"
    );
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_memory_mode_multiple_recordings() -> Result<(), Box<dyn std::error::Error>> {
    // デーモン起動（メモリモード）
    let daemon = start_voice_inputd()?;
    wait_for_daemon_ready()?;
    
    // 複数回の録音を実行
    for i in 1..=3 {
        println!("Recording iteration {}", i);
        
        // 録音開始
        let result = voice_input_cli(&["toggle"]).await?;
        assert!(
            result.contains("Recording started") || result.contains("録音を開始"),
            "Expected recording start message on iteration {}",
            i
        );
        
        // 音声シミュレーション（2秒）
        simulate_audio_input(Duration::from_secs(2)).await?;
        
        // 録音停止と転写
        let result = voice_input_cli(&["toggle"]).await?;
        assert!(
            result.contains("Transcription:") || result.contains("転写結果:"),
            "Expected transcription result on iteration {}",
            i
        );
        
        // 次の録音まで少し待機
        thread::sleep(Duration::from_secs(1));
    }
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_memory_mode_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    // デーモン起動（メモリモード）
    let daemon = start_voice_inputd()?;
    wait_for_daemon_ready()?;
    
    // 録音開始
    voice_input_cli(&["toggle"]).await?;
    
    // 録音中に無効なコマンドを送信
    let result = voice_input_cli(&["invalid_command"]).await;
    assert!(result.is_err() || result.unwrap().contains("error"));
    
    // 録音は継続しているはず
    simulate_audio_input(Duration::from_secs(2)).await?;
    
    // 録音停止
    let result = voice_input_cli(&["toggle"]).await?;
    assert!(
        result.contains("Transcription:") || result.contains("転写結果:"),
        "Recording should continue despite invalid command"
    );
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}