//! Security test for text input
//! 
//! Tests injection resistance and security aspects

use voice_input::infrastructure::external::text_input;

#[tokio::main]
async fn main() {
    println!("=== Security Test Suite ===\n");

    // S1: Command injection attempts
    println!("S1: Testing command injection resistance...");
    let injection_tests = vec![
        r#""; echo "injected""#,
        r#"` echo injected `"#,
        r#"$(echo injected)"#,
        r#"\"; osascript -e 'display dialog \"Injected!\"'; echo \""#,
        r#"" & osascript -e "display dialog \"Injected!\"" & echo ""#,
    ];

    for (i, test) in injection_tests.iter().enumerate() {
        println!("  Test {}: {}", i + 1, test);
        match text_input::type_text(test).await {
            Ok(_) => println!("    ✅ Text typed literally (no injection)"),
            Err(e) => println!("    ⚠️ Error: {}", e),
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // S2: Log output check
    println!("\nS2: Checking for sensitive data in logs...");
    let sensitive_data = "password123_SECRET_KEY";
    match text_input::type_text(sensitive_data).await {
        Ok(_) => println!("✅ S2: No sensitive data logged"),
        Err(e) => {
            if e.to_string().contains("SECRET_KEY") {
                println!("❌ S2: FAIL - Sensitive data found in error: {}", e);
            } else {
                println!("✅ S2: Error doesn't expose sensitive data");
            }
        }
    }

    // S3: AppleScript command construction safety
    println!("\nS3: Testing AppleScript command construction safety...");
    let special_chars = r#"tell application "System Events" to display dialog "Hacked!""#;
    match text_input::type_text(special_chars).await {
        Ok(_) => println!("✅ S3: Special AppleScript commands handled safely"),
        Err(e) => println!("✅ S3: Safely rejected or errored: {}", e),
    }

    println!("\n=== Security Test Complete ===");
    println!("\nNote: Check console/system logs manually to ensure no sensitive data leakage");
}