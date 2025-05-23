//! Performance test for text input
//!
//! Measures the time taken for various text input operations

use std::time::Instant;
use voice_input::infrastructure::external::text_input;

#[tokio::main]
async fn main() {
    println!("=== Text Input Performance Test ===\n");

    // P1: Short text (5 words)
    let short_text = "Hello world from Rust programming";
    println!("P1: Short text ({} chars)", short_text.len());
    let start = Instant::now();
    match text_input::type_text(short_text).await {
        Ok(_) => {
            let elapsed = start.elapsed();
            println!("✅ P1: Completed in {:?} (target: < 0.5s)", elapsed);
            if elapsed.as_secs_f64() < 0.5 {
                println!("   PASS: Within target time");
            } else {
                println!("   FAIL: Exceeded target time");
            }
        }
        Err(e) => println!("❌ P1 Error: {}", e),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // P2: Medium text (50 words)
    let medium_text = "The quick brown fox jumps over the lazy dog. This pangram sentence contains every letter of the English alphabet at least once. It is commonly used for testing typefaces, keyboards, and other applications involving text display or input. The sentence has been used since at least the late 1800s.";
    println!("\nP2: Medium text ({} chars)", medium_text.len());
    let start = Instant::now();
    match text_input::type_text(medium_text).await {
        Ok(_) => {
            let elapsed = start.elapsed();
            println!("✅ P2: Completed in {:?} (target: < 2s)", elapsed);
            if elapsed.as_secs_f64() < 2.0 {
                println!("   PASS: Within target time");
            } else {
                println!("   FAIL: Exceeded target time");
            }
        }
        Err(e) => println!("❌ P2 Error: {}", e),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // P3: Long text (200 words)
    let long_text = medium_text.repeat(4);
    println!("\nP3: Long text ({} chars)", long_text.len());
    let start = Instant::now();
    match text_input::type_text(&long_text).await {
        Ok(_) => {
            let elapsed = start.elapsed();
            println!("✅ P3: Completed in {:?} (target: < 10s)", elapsed);
            if elapsed.as_secs_f64() < 10.0 {
                println!("   PASS: Within target time");
            } else {
                println!("   FAIL: Exceeded target time");
            }
        }
        Err(e) => println!("❌ P3 Error: {}", e),
    }

    println!("\n=== Performance Test Complete ===");
    println!("\nNote: P4 (CPU usage) and P5 (memory leaks) should be monitored externally");
    println!("using tools like Activity Monitor or instruments.");
}
