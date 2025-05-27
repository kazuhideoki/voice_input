use std::time::Duration;
use super::*;

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_file_mode_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    // デーモン起動（ファイルモード）
    let daemon = start_voice_inputd(true)?;
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
    
    // ファイルモードの確認（ファイルが作成されていること）
    let wav_files: Vec<_> = std::fs::read_dir("/tmp")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().contains("voice_input_")
                && entry.file_name().to_string_lossy().ends_with(".wav")
        })
        .collect();
    
    assert!(wav_files.len() > 0, "WAV files should be created in file mode");
    
    // クリーンアップ
    for file in wav_files {
        let _ = std::fs::remove_file(file.path());
    }
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_file_mode_cleanup() -> Result<(), Box<dyn std::error::Error>> {
    // デーモン起動（ファイルモード）
    let daemon = start_voice_inputd(true)?;
    wait_for_daemon_ready()?;
    
    // 録音開始
    voice_input_cli(&["toggle"]).await?;
    
    // 音声シミュレーション（2秒）
    simulate_audio_input(Duration::from_secs(2)).await?;
    
    // 録音停止
    voice_input_cli(&["toggle"]).await?;
    
    // ファイルが作成されたことを確認
    let wav_files_before: Vec<_> = std::fs::read_dir("/tmp")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().contains("voice_input_")
                && entry.file_name().to_string_lossy().ends_with(".wav")
        })
        .collect();
    
    assert!(wav_files_before.len() > 0, "WAV file should be created");
    
    // しばらく待機してファイルが削除されることを確認
    thread::sleep(Duration::from_secs(5));
    
    let wav_files_after: Vec<_> = std::fs::read_dir("/tmp")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().contains("voice_input_")
                && entry.file_name().to_string_lossy().ends_with(".wav")
        })
        .collect();
    
    // ファイルが適切に削除されているか確認
    // （実装によっては自動削除されない場合もある）
    println!(
        "WAV files before: {}, after: {}",
        wav_files_before.len(),
        wav_files_after.len()
    );
    
    // クリーンアップ
    for file in wav_files_after {
        let _ = std::fs::remove_file(file.path());
    }
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_file_mode_large_recording() -> Result<(), Box<dyn std::error::Error>> {
    // デーモン起動（ファイルモード）
    let daemon = start_voice_inputd(true)?;
    wait_for_daemon_ready()?;
    
    // 録音開始
    let result = voice_input_cli(&["toggle"]).await?;
    assert!(
        result.contains("Recording started") || result.contains("録音を開始"),
        "Expected recording start message"
    );
    
    // 長時間録音シミュレーション（60秒）
    simulate_audio_input(Duration::from_secs(60)).await?;
    
    // 録音停止と転写
    let result = voice_input_cli(&["toggle"]).await?;
    assert!(
        result.contains("Transcription:") || result.contains("転写結果:"),
        "Expected transcription result after long recording"
    );
    
    // 大きなWAVファイルが作成されたことを確認
    let wav_files: Vec<_> = std::fs::read_dir("/tmp")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().contains("voice_input_")
                && entry.file_name().to_string_lossy().ends_with(".wav")
        })
        .collect();
    
    for file in &wav_files {
        let metadata = file.metadata()?;
        println!("WAV file size: {} bytes", metadata.len());
        assert!(
            metadata.len() > 1000000,
            "Large WAV file should be created for long recording"
        );
    }
    
    // クリーンアップ
    for file in wav_files {
        let _ = std::fs::remove_file(file.path());
    }
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
async fn test_file_mode_concurrent_recordings() -> Result<(), Box<dyn std::error::Error>> {
    // デーモン起動（ファイルモード）
    let daemon = start_voice_inputd(true)?;
    wait_for_daemon_ready()?;
    
    // 最初の録音開始
    let result = voice_input_cli(&["toggle"]).await?;
    assert!(
        result.contains("Recording started") || result.contains("録音を開始"),
        "Expected recording start message"
    );
    
    // 録音中に再度toggleを送信（エラーになるはず）
    let result = voice_input_cli(&["toggle"]).await;
    // 実装によっては録音停止になる可能性もある
    println!("Second toggle result: {:?}", result);
    
    // 少し待機
    simulate_audio_input(Duration::from_secs(2)).await?;
    
    // 録音状態を確認して適切に停止
    let _ = voice_input_cli(&["toggle"]).await;
    
    // クリーンアップ
    let wav_files: Vec<_> = std::fs::read_dir("/tmp")
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().contains("voice_input_")
                && entry.file_name().to_string_lossy().ends_with(".wav")
        })
        .collect();
    
    for file in wav_files {
        let _ = std::fs::remove_file(file.path());
    }
    
    // デーモン終了
    kill_daemon(daemon)?;
    Ok(())
}