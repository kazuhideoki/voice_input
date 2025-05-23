//! Text input demonstration
//!
//! This example demonstrates the direct text input functionality
//! using AppleScript keystroke feature.
//!
//! Note: This requires accessibility permissions for Terminal.

use voice_input::infrastructure::external::text_input;

#[tokio::main]
async fn main() {
    println!("=== Text Input Demo ===");
    println!("Note: Requires accessibility permissions for Terminal\n");

    // T1: Basic English text
    println!("T1: Testing basic English text...");
    match text_input::type_text("Hello World").await {
        Ok(_) => println!("✅ T1 Success: Basic English text"),
        Err(e) => println!("❌ T1 Error: {}", e),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // T2: Japanese text
    println!("\nT2: Testing Japanese text...");
    match text_input::type_text("こんにちは世界").await {
        Ok(_) => println!("✅ T2 Success: Japanese text"),
        Err(e) => println!("❌ T2 Error: {}", e),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // T3: Special characters
    println!("\nT3: Testing special characters...");
    match text_input::type_text("Test \"quotes\" and \\backslash").await {
        Ok(_) => println!("✅ T3 Success: Special characters"),
        Err(e) => println!("❌ T3 Error: {}", e),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // T4: Newline
    println!("\nT4: Testing newline...");
    match text_input::type_text("Line1\nLine2").await {
        Ok(_) => println!("✅ T4 Success: Newline handling"),
        Err(e) => println!("❌ T4 Error: {}", e),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // T5: Empty string
    println!("\nT5: Testing empty string...");
    match text_input::type_text("").await {
        Ok(_) => println!("✅ T5 Success: Empty string"),
        Err(e) => println!("❌ T5 Error: {}", e),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // T6: 300 character text (chunked)
    println!("\nT6: Testing 300 character text (chunked)...");
    let text_300 = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum.";
    let start = std::time::Instant::now();
    match text_input::type_text(text_300).await {
        Ok(_) => println!("✅ T6 Success: 300 char text in {:?}", start.elapsed()),
        Err(e) => println!("❌ T6 Error: {}", e),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // T7: 1000 character text
    println!("\nT7: Testing 1000 character text...");
    let text_1000 = text_300.repeat(3) + " End of test.";
    let start = std::time::Instant::now();
    match text_input::type_text(&text_1000).await {
        Ok(_) => println!("✅ T7 Success: 1000 char text in {:?}", start.elapsed()),
        Err(e) => println!("❌ T7 Error: {}", e),
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // T10: Config validation
    println!("\nT10: Testing config validation...");
    let mut config = text_input::TextInputConfig::default();
    config.max_chunk_size = 0;
    match text_input::validate_config(&config) {
        Ok(_) => println!("❌ T10 Failed: Should have rejected invalid config"),
        Err(e) => println!("✅ T10 Success: Config validation error: {}", e),
    }

    println!("\n=== Test Suite Complete ===");
}
