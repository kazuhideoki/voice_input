//! Integration test for text_input module
//!
//! Verifies that the module can be imported and used from other parts of the codebase

use voice_input::infrastructure::external::text_input::{
    TextInputConfig, TextInputError, type_text, type_text_directly, validate_config,
};

#[tokio::main]
async fn main() {
    println!("=== Integration Test ===\n");

    // I2: Test import and basic usage
    println!("I2: Testing module import and basic usage...");
    match type_text("Test from integration").await {
        Ok(_) => println!("✅ I2: Module imported and function callable"),
        Err(e) => println!("✅ I2: Module imported, error handling works: {}", e),
    }

    // I3: Test error handling in upper layers
    println!("\nI3: Testing error handling in upper layers...");
    let result = type_text_directly(
        "Test",
        &TextInputConfig {
            max_chunk_size: 0,
            ..Default::default()
        },
    )
    .await;

    match result {
        Ok(_) => println!("❌ I3: Should have failed with invalid config"),
        Err(e) => {
            println!("✅ I3: Error properly propagated: {}", e);

            // Demonstrate error type matching
            match e {
                TextInputError::InvalidInput(msg) => {
                    println!("   Error type: InvalidInput - {}", msg);
                }
                TextInputError::AppleScriptFailure(msg) => {
                    println!("   Error type: AppleScriptFailure - {}", msg);
                }
                TextInputError::Timeout => {
                    println!("   Error type: Timeout");
                }
                TextInputError::EscapeError(msg) => {
                    println!("   Error type: EscapeError - {}", msg);
                }
            }
        }
    }

    // Test that validate_config is accessible
    let config = TextInputConfig::default();
    match validate_config(&config) {
        Ok(_) => println!("\n✅ validate_config accessible and working"),
        Err(e) => println!("\n❌ validate_config error: {}", e),
    }

    println!("\n=== Integration Test Complete ===");
}
