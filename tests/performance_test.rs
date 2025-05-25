use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use voice_input::infrastructure::external::text_input::type_text;

/// パフォーマンステスト結果
#[derive(Debug)]
struct PerformanceResult {
    text_length: usize,
    direct_input_time: Option<Duration>,
    paste_time: Option<Duration>,
    direct_input_error: Option<String>,
    paste_error: Option<String>,
}

/// クリップボードにテキストを設定
async fn set_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes()).await?;
    }

    child.wait().await?;
    Ok(())
}

/// ペースト方式でテキストを入力（シミュレーション）
async fn paste_text(text: &str) -> Result<Duration, Box<dyn std::error::Error>> {
    let start = Instant::now();

    // クリップボードに設定
    set_clipboard(text).await?;

    // Cmd+Vをシミュレート
    let output = Command::new("osascript")
        .arg("-e")
        .arg(r#"tell application "System Events" to keystroke "v" using {command down}"#)
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Paste command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(start.elapsed())
}

/// 直接入力方式でテキストを入力
async fn direct_input_text(text: &str) -> Result<Duration, Box<dyn std::error::Error>> {
    let start = Instant::now();
    type_text(text).await?;
    Ok(start.elapsed())
}

/// パフォーマンステストを実行
async fn run_performance_test(text: &str, description: &str) -> PerformanceResult {
    println!("\n=== {} ===", description);
    println!("Text length: {} characters", text.len());

    let mut result = PerformanceResult {
        text_length: text.len(),
        direct_input_time: None,
        paste_time: None,
        direct_input_error: None,
        paste_error: None,
    };

    // 直接入力のテスト
    print!("Testing direct input... ");
    match direct_input_text(text).await {
        Ok(duration) => {
            println!("✓ {:.2}s", duration.as_secs_f64());
            result.direct_input_time = Some(duration);
        }
        Err(e) => {
            println!("✗ Error: {}", e);
            result.direct_input_error = Some(e.to_string());
        }
    }

    // 少し待つ
    tokio::time::sleep(Duration::from_secs(1)).await;

    // ペースト方式のテスト
    print!("Testing paste method... ");
    match paste_text(text).await {
        Ok(duration) => {
            println!("✓ {:.2}s", duration.as_secs_f64());
            result.paste_time = Some(duration);
        }
        Err(e) => {
            println!("✗ Error: {}", e);
            result.paste_error = Some(e.to_string());
        }
    }

    result
}

#[tokio::test]
#[ignore] // 手動実行用：cargo test --test performance_test -- --ignored --nocapture
async fn benchmark_direct_vs_paste() -> Result<(), Box<dyn std::error::Error>> {
    println!("Voice Input Performance Benchmark");
    println!("=================================");
    println!("Comparing direct input vs paste method");
    println!("\nNOTE: Open a text editor and place cursor in a text field before running!");
    println!("Waiting 5 seconds...");

    tokio::time::sleep(Duration::from_secs(5)).await;

    let long_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(20);
    let test_cases = vec![
        ("Short text", "Hello, World!"),
        (
            "Medium text",
            "The quick brown fox jumps over the lazy dog. This is a test of the voice input system.",
        ),
        ("Long text", long_text.as_str()),
        (
            "Japanese text",
            "こんにちは、世界！日本語のテキスト入力テストです。",
        ),
        (
            "Mixed content",
            "Test 123! 特殊文字 @#$% と絵文字 🎉 を含むテキスト。\n改行も\nテストします。",
        ),
    ];

    let mut results = Vec::new();

    for (description, text) in test_cases {
        let result = run_performance_test(text, description).await;
        results.push(result);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // レポート生成
    println!("\n\n=== Performance Report ===");
    println!(
        "{:<15} {:<10} {:<15} {:<15} {:<10}",
        "Test", "Length", "Direct (s)", "Paste (s)", "Diff"
    );
    println!("{}", "-".repeat(70));

    for (i, result) in results.iter().enumerate() {
        let test_name = match i {
            0 => "Short",
            1 => "Medium",
            2 => "Long",
            3 => "Japanese",
            4 => "Mixed",
            _ => "Unknown",
        };

        let direct_time = result
            .direct_input_time
            .map(|d| format!("{:.3}", d.as_secs_f64()))
            .unwrap_or_else(|| "Error".to_string());

        let paste_time = result
            .paste_time
            .map(|d| format!("{:.3}", d.as_secs_f64()))
            .unwrap_or_else(|| "Error".to_string());

        let diff = match (result.direct_input_time, result.paste_time) {
            (Some(d), Some(p)) => {
                let diff_ms = d.as_millis() as i64 - p.as_millis() as i64;
                if diff_ms > 0 {
                    format!("+{:.3}s", diff_ms as f64 / 1000.0)
                } else {
                    format!("{:.3}s", diff_ms as f64 / 1000.0)
                }
            }
            _ => "N/A".to_string(),
        };

        println!(
            "{:<15} {:<10} {:<15} {:<15} {:<10}",
            test_name, result.text_length, direct_time, paste_time, diff
        );
    }

    println!("\n=== Summary ===");

    // 平均時間の計算
    let direct_times: Vec<_> = results.iter().filter_map(|r| r.direct_input_time).collect();

    let paste_times: Vec<_> = results.iter().filter_map(|r| r.paste_time).collect();

    if !direct_times.is_empty() {
        let avg_direct =
            direct_times.iter().map(|d| d.as_secs_f64()).sum::<f64>() / direct_times.len() as f64;
        println!("Average direct input time: {:.3}s", avg_direct);
    }

    if !paste_times.is_empty() {
        let avg_paste =
            paste_times.iter().map(|d| d.as_secs_f64()).sum::<f64>() / paste_times.len() as f64;
        println!("Average paste time: {:.3}s", avg_paste);
    }

    // エラー報告
    let errors: Vec<_> = results
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            if r.direct_input_error.is_some() || r.paste_error.is_some() {
                Some((i, r))
            } else {
                None
            }
        })
        .collect();

    if !errors.is_empty() {
        println!("\n=== Errors ===");
        for (i, result) in errors {
            if let Some(err) = &result.direct_input_error {
                println!("Test {}: Direct input error: {}", i + 1, err);
            }
            if let Some(err) = &result.paste_error {
                println!("Test {}: Paste error: {}", i + 1, err);
            }
        }
    }

    println!("\n=== Recommendations ===");
    if direct_times.len() == paste_times.len() && !direct_times.is_empty() {
        let avg_direct =
            direct_times.iter().map(|d| d.as_secs_f64()).sum::<f64>() / direct_times.len() as f64;
        let avg_paste =
            paste_times.iter().map(|d| d.as_secs_f64()).sum::<f64>() / paste_times.len() as f64;

        let diff_percent = ((avg_direct - avg_paste) / avg_paste * 100.0).abs();

        if avg_direct < avg_paste {
            println!(
                "✓ Direct input is {:.1}% faster than paste method",
                diff_percent
            );
            println!("✓ Recommend using direct input as default");
        } else if diff_percent < 20.0 {
            println!("✓ Performance difference is minimal ({:.1}%)", diff_percent);
            println!("✓ Direct input provides clipboard preservation benefit");
            println!("✓ Recommend using direct input as default");
        } else {
            println!(
                "⚠ Direct input is {:.1}% slower than paste method",
                diff_percent
            );
            println!("⚠ Consider performance vs clipboard preservation trade-off");
        }
    }

    Ok(())
}
