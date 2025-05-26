//! パフォーマンステスト
//! メモリモードとファイルモードの性能比較を行います。
//!
//! ## 実行前要件
//! 1. OpenAI APIキーの設定:
//!    ```bash
//!    export OPENAI_API_KEY="your_api_key_here"
//!    ```
//! 2. 音声入力デバイスが利用可能であること
//!    ```bash
//!    cargo run --bin voice_inputd &
//!    cargo run --bin voice_input -- --list-devices
//!    pkill voice_inputd
//!    ```
//!
//! ## 実行方法
//! ```bash
//! # 環境変数を設定してからテスト実行
//! export OPENAI_API_KEY="your_api_key_here"
//! cargo test --test performance_test -- --ignored --nocapture
//! ```

mod benchmarks;

use std::env;
use std::error::Error;
use std::thread;
use std::time::{Duration, Instant};
use voice_input::domain::recorder::Recorder;
use voice_input::infrastructure::audio::cpal_backend::CpalAudioBackend;
use voice_input::infrastructure::external::openai::OpenAiClient;

#[derive(Debug)]
struct PerformanceMetrics {
    recording_time: Duration,
    transcription_time: Duration,
    total_time: Duration,
    #[allow(dead_code)]
    memory_usage_mb: f64,
    #[allow(dead_code)]
    mode: String,
}

/// 現在のメモリ使用量を取得（簡易実装）
fn get_current_memory_usage_mb() -> f64 {
    // macOSでは正確なメモリ使用量の取得は困難なため、ダミー値を返す
    // 実際の実装では、システムコールやプロセス情報を使用
    0.0
}

/// パフォーマンスを測定
async fn measure_performance(use_legacy: bool) -> Result<PerformanceMetrics, Box<dyn Error>> {
    // 環境変数設定
    unsafe {
        if use_legacy {
            env::set_var("LEGACY_TMP_WAV_FILE", "true");
        } else {
            env::remove_var("LEGACY_TMP_WAV_FILE");
        }
    }

    let start = Instant::now();

    // 録音開始
    let backend = CpalAudioBackend::default();
    let mut recorder = Recorder::new(backend);
    recorder.start()?;

    // 5秒間録音
    thread::sleep(Duration::from_secs(5));

    let recording_end = Instant::now();
    let audio_data = recorder.stop_raw()?;

    // OpenAI API呼び出し
    let client = OpenAiClient::new()?;
    let transcription_start = Instant::now();
    let _result = client.transcribe_audio(audio_data).await?;

    let total_end = Instant::now();

    Ok(PerformanceMetrics {
        recording_time: recording_end - start,
        transcription_time: total_end - transcription_start,
        total_time: total_end - start,
        memory_usage_mb: get_current_memory_usage_mb(),
        mode: if use_legacy {
            "File".to_string()
        } else {
            "Memory".to_string()
        },
    })
}

/// 結果を表形式で出力
fn print_results(memory_metrics: &PerformanceMetrics, file_metrics: &PerformanceMetrics) {
    println!("\n🎯 Performance Comparison Results");
    println!("═══════════════════════════════════════════════════════════════");
    println!(
        "{:<20} │ {:>15} │ {:>15} │ {:>10}",
        "Metric", "Memory Mode", "File Mode", "Difference"
    );
    println!("───────────────────────────────────────────────────────────────");

    // 録音時間
    println!(
        "{:<20} │ {:>13.2}ms │ {:>13.2}ms │ {:>8.2}ms",
        "Recording Time",
        memory_metrics.recording_time.as_millis(),
        file_metrics.recording_time.as_millis(),
        memory_metrics.recording_time.as_millis() as f64
            - file_metrics.recording_time.as_millis() as f64
    );

    // 転写時間
    println!(
        "{:<20} │ {:>13.2}ms │ {:>13.2}ms │ {:>8.2}ms",
        "Transcription Time",
        memory_metrics.transcription_time.as_millis(),
        file_metrics.transcription_time.as_millis(),
        memory_metrics.transcription_time.as_millis() as f64
            - file_metrics.transcription_time.as_millis() as f64
    );

    // 合計時間
    println!(
        "{:<20} │ {:>13.2}ms │ {:>13.2}ms │ {:>8.2}ms",
        "Total Time",
        memory_metrics.total_time.as_millis(),
        file_metrics.total_time.as_millis(),
        memory_metrics.total_time.as_millis() as f64 - file_metrics.total_time.as_millis() as f64
    );

    println!("═══════════════════════════════════════════════════════════════");

    // パフォーマンス改善率
    let improvement = ((file_metrics.total_time.as_millis() as f64
        - memory_metrics.total_time.as_millis() as f64)
        / file_metrics.total_time.as_millis() as f64)
        * 100.0;

    if improvement > 0.0 {
        println!(
            "\n✅ Performance Improvement: {:.1}% faster in Memory mode",
            improvement
        );
    } else {
        println!(
            "\n⚠️  Performance Degradation: {:.1}% slower in Memory mode",
            -improvement
        );
    }
}

#[tokio::test]
#[ignore]
async fn test_performance_comparison() {
    // OpenAI APIキーが設定されているか確認
    if env::var("OPENAI_API_KEY").is_err() {
        eprintln!("⚠️  OPENAI_API_KEY not set. Skipping performance test.");
        return;
    }

    println!("🚀 Starting performance comparison test...");
    println!("This test will record 5 seconds of audio in each mode.\n");

    // メモリモードでの測定
    println!("📊 Testing Memory mode...");
    let memory_metrics = match measure_performance(false).await {
        Ok(metrics) => metrics,
        Err(e) => {
            eprintln!("❌ Memory mode test failed: {}", e);
            return;
        }
    };

    // 少し待機
    thread::sleep(Duration::from_secs(2));

    // ファイルモードでの測定
    println!("📊 Testing File mode...");
    let file_metrics = match measure_performance(true).await {
        Ok(metrics) => metrics,
        Err(e) => {
            eprintln!("❌ File mode test failed: {}", e);
            return;
        }
    };

    // 結果を表示
    print_results(&memory_metrics, &file_metrics);
}

#[tokio::test]
#[ignore]
async fn test_memory_usage() {
    println!("\n🧪 Memory Usage Test");
    println!("Testing memory consumption with longer recording...\n");

    // 30秒録音でのメモリ使用量を確認
    unsafe {
        env::remove_var("LEGACY_TMP_WAV_FILE");
    }

    let backend = CpalAudioBackend::default();

    let mut recorder = Recorder::new(backend);

    println!("🎙️  Recording for 30 seconds...");
    if let Err(e) = recorder.start() {
        eprintln!("❌ Failed to start recording: {}", e);
        return;
    }

    // 30秒録音
    thread::sleep(Duration::from_secs(30));

    match recorder.stop_raw() {
        Ok(audio_data) => {
            match audio_data {
                voice_input::infrastructure::audio::cpal_backend::AudioData::Memory(data) => {
                    let size_mb = data.len() as f64 / (1024.0 * 1024.0);
                    println!("✅ Memory mode - WAV data size: {:.2} MB", size_mb);

                    // 理論値との比較
                    // 48kHz * 2ch * 2bytes * 30sec = 5.76MB
                    let expected_mb = 48000.0 * 2.0 * 2.0 * 30.0 / (1024.0 * 1024.0);
                    println!("📐 Expected size (theoretical): {:.2} MB", expected_mb);
                    println!(
                        "📊 Actual vs Expected: {:.1}%",
                        (size_mb / expected_mb) * 100.0
                    );
                }
                voice_input::infrastructure::audio::cpal_backend::AudioData::File(path) => {
                    println!("📁 File mode - saved to: {:?}", path);
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to stop recording: {}", e);
        }
    }
}
