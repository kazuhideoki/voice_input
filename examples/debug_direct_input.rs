use voice_input::infrastructure::external::text_input::{type_text, TextInputConfig};

#[tokio::main]
async fn main() {
    println!("Direct input debug test");
    println!("======================");
    
    let test_text = "Hello, World!";
    println!("Input text: '{}'", test_text);
    println!("Length: {} characters", test_text.len());
    
    // デフォルト設定で実行
    println!("\nAttempting direct input...");
    match type_text(test_text).await {
        Ok(_) => println!("✓ Direct input succeeded"),
        Err(e) => println!("✗ Direct input failed: {}", e),
    }
    
    // AppleScriptコマンドを直接確認
    let escaped = test_text
        .replace("\\", "\\\\")
        .replace("\"", "\\\"")
        .replace("\n", "\r");
    
    println!("\nEscaped text: '{}'", escaped);
    println!("AppleScript command would be:");
    println!(r#"tell application "System Events" to keystroke "{}""#, escaped);
}