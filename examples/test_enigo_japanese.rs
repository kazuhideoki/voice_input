use voice_input::infrastructure::external::text_input_enigo::type_text_with_enigo;

#[tokio::main]
async fn main() {
    println!("Enigo Japanese input test");
    println!("========================");
    println!("Please open a text editor and place cursor in a text field");
    println!("Starting in 3 seconds...");

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let test_texts = vec![
        ("English", "Hello, World!"),
        ("Japanese", "こんにちは、世界！"),
        ("Mixed", "Test 123 テスト"),
        ("Emoji", "絵文字テスト 🎉 🚀"),
    ];

    for (label, text) in test_texts {
        println!("\nTesting {}: '{}'", label, text);
        match type_text_with_enigo(text).await {
            Ok(_) => println!("✓ Success"),
            Err(e) => println!("✗ Error: {}", e),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    println!("\nTest completed!");
}
