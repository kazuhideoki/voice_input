use std::time::{Duration, Instant};
use voice_input::domain::recorder::Recorder;
use voice_input::infrastructure::audio::CpalAudioBackend;
use voice_input::monitoring::MemoryMonitor;
use std::sync::Arc;

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn benchmark_memory_mode_recording() {
    let backend = CpalAudioBackend::default();
    let monitor = Arc::new(MemoryMonitor::new(100)); // 100MB threshold
    let mut recorder = Recorder::new(backend).with_memory_monitor(monitor.clone());
    
    // 測定開始
    let start = Instant::now();
    
    // 録音開始
    recorder.start().expect("Failed to start recording");
    
    // 5秒間録音
    std::thread::sleep(Duration::from_secs(5));
    
    // 録音停止
    let audio_data = recorder.stop_raw().expect("Failed to stop recording");
    
    let elapsed = start.elapsed();
    
    // 結果を出力
    println!("Recording benchmark (memory mode):");
    println!("  Total time: {:?}", elapsed);
    println!("  Memory metrics: {:?}", monitor.get_metrics());
    
    match audio_data {
        voice_input::infrastructure::audio::AudioData::Memory(data) => {
            println!("  Audio data size: {} bytes ({:.2} MB)", 
                     data.len(), 
                     data.len() as f64 / 1024.0 / 1024.0);
        }
        _ => panic!("Expected memory mode"),
    }
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn benchmark_file_mode_recording() {
    // ファイルモードに設定
    unsafe {
        std::env::set_var("LEGACY_TMP_WAV_FILE", "true");
    }
    
    let backend = CpalAudioBackend::default();
    let mut recorder = Recorder::new(backend);
    
    // 測定開始
    let start = Instant::now();
    
    // 録音開始
    recorder.start().expect("Failed to start recording");
    
    // 5秒間録音
    std::thread::sleep(Duration::from_secs(5));
    
    // 録音停止
    let audio_data = recorder.stop_raw().expect("Failed to stop recording");
    
    let elapsed = start.elapsed();
    
    // 結果を出力
    println!("Recording benchmark (file mode):");
    println!("  Total time: {:?}", elapsed);
    
    match audio_data {
        voice_input::infrastructure::audio::AudioData::File(path) => {
            println!("  Audio file path: {:?}", path);
            if let Ok(metadata) = std::fs::metadata(&path) {
                println!("  File size: {} bytes ({:.2} MB)", 
                         metadata.len(), 
                         metadata.len() as f64 / 1024.0 / 1024.0);
            }
            // クリーンアップ
            let _ = std::fs::remove_file(path);
        }
        _ => panic!("Expected file mode"),
    }
    
    // 環境変数をリセット
    unsafe {
        std::env::remove_var("LEGACY_TMP_WAV_FILE");
    }
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn compare_recording_modes() {
    println!("\n=== Comparing Recording Modes ===\n");
    
    let durations = vec![1, 3, 5];
    let mut results = Vec::new();
    
    for duration_secs in durations {
        println!("Testing {}s recording...", duration_secs);
        
        // メモリモード
        let backend = CpalAudioBackend::default();
        let monitor = Arc::new(MemoryMonitor::new(200));
        let mut recorder = Recorder::new(backend).with_memory_monitor(monitor.clone());
        
        let memory_start = Instant::now();
        recorder.start().unwrap();
        std::thread::sleep(Duration::from_secs(duration_secs));
        let memory_data = recorder.stop_raw().unwrap();
        let memory_elapsed = memory_start.elapsed();
        
        let memory_size = match &memory_data {
            voice_input::infrastructure::audio::AudioData::Memory(data) => data.len(),
            _ => 0,
        };
        
        // ファイルモード
        unsafe {
            std::env::set_var("LEGACY_TMP_WAV_FILE", "true");
        }
        let backend = CpalAudioBackend::default();
        let mut recorder = Recorder::new(backend);
        
        let file_start = Instant::now();
        recorder.start().unwrap();
        std::thread::sleep(Duration::from_secs(duration_secs));
        let file_data = recorder.stop_raw().unwrap();
        let file_elapsed = file_start.elapsed();
        
        let file_size = match &file_data {
            voice_input::infrastructure::audio::AudioData::File(path) => {
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as usize;
                let _ = std::fs::remove_file(path);
                size
            }
            _ => 0,
        };
        
        unsafe {
            std::env::remove_var("LEGACY_TMP_WAV_FILE");
        }
        
        // 結果を記録
        results.push((
            duration_secs,
            memory_elapsed,
            memory_size,
            file_elapsed,
            file_size,
        ));
        
        println!("  Memory mode: {:?} ({} bytes)", memory_elapsed, memory_size);
        println!("  File mode: {:?} ({} bytes)", file_elapsed, file_size);
        println!();
    }
    
    // サマリー
    println!("\n=== Performance Summary ===");
    println!("{:<10} {:<20} {:<20} {:<20}", "Duration", "Memory Time", "File Time", "Speedup");
    println!("{:-<70}", "");
    
    for (duration, mem_time, _, file_time, _) in results {
        let speedup = file_time.as_secs_f64() / mem_time.as_secs_f64();
        println!(
            "{:<10} {:<20} {:<20} {:.2}x",
            format!("{}s", duration),
            format!("{:?}", mem_time),
            format!("{:?}", file_time),
            speedup
        );
    }
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn benchmark_memory_monitor_overhead() {
    println!("\n=== Memory Monitor Overhead Test ===\n");
    
    // 監視なし
    let backend_no_monitor = CpalAudioBackend::default();
    let mut recorder_no_monitor = Recorder::new(backend_no_monitor);
    let start = Instant::now();
    recorder_no_monitor.start().unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let _ = recorder_no_monitor.stop_raw().unwrap();
    let elapsed_no_monitor = start.elapsed();
    
    // 監視あり
    let backend_with_monitor = CpalAudioBackend::default();
    let monitor = Arc::new(MemoryMonitor::new(100));
    let mut recorder_with_monitor = Recorder::new(backend_with_monitor).with_memory_monitor(monitor.clone());
    let start = Instant::now();
    recorder_with_monitor.start().unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let _ = recorder_with_monitor.stop_raw().unwrap();
    let elapsed_with_monitor = start.elapsed();
    
    let overhead = elapsed_with_monitor.as_secs_f64() - elapsed_no_monitor.as_secs_f64();
    let overhead_percent = (overhead / elapsed_no_monitor.as_secs_f64()) * 100.0;
    
    println!("Without monitor: {:?}", elapsed_no_monitor);
    println!("With monitor: {:?}", elapsed_with_monitor);
    println!("Overhead: {:.3}ms ({:.2}%)", overhead * 1000.0, overhead_percent);
    println!("Monitor metrics: {:?}", monitor.get_metrics());
    
    // オーバーヘッドが1%未満であることを確認
    assert!(
        overhead_percent < 1.0,
        "Memory monitor overhead too high: {:.2}%",
        overhead_percent
    );
}