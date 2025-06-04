//! text_input_accessibility モジュールの単体テスト

use voice_input::infrastructure::external::text_input_accessibility::{
    check_accessibility_permission, check_focused_element_is_text_field, TextInputError,
};

#[test]
fn test_error_types() {
    // エラー型が正しく定義されているか確認
    let error = TextInputError::NoFocusedElement;
    assert_eq!(format!("{}", error), "No focused element found");

    let error = TextInputError::NotTextElement;
    assert_eq!(format!("{}", error), "Focused element is not a text field");

    let error = TextInputError::PermissionDenied;
    assert!(format!("{}", error).contains("System Settings"));

    let error = TextInputError::ApiCallFailed("Test error".to_string());
    assert!(format!("{}", error).contains("Test error"));

    let error = TextInputError::CursorPositionError("Cursor error".to_string());
    assert!(format!("{}", error).contains("Cursor error"));
}

#[test]
#[ignore] // 手動実行用: アクセシビリティ権限が必要
fn test_check_accessibility_permission() {
    match check_accessibility_permission() {
        Ok(()) => {
            println!("✅ Accessibility permission granted");
        }
        Err(TextInputError::PermissionDenied) => {
            println!("❌ Accessibility permission denied - this is expected if not granted");
        }
        Err(e) => {
            panic!("Unexpected error: {}", e);
        }
    }
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
#[ignore] // 手動実行用: 実際のAPI呼び出しをテスト
async fn test_insert_text_at_cursor_basic() {
    use voice_input::infrastructure::external::text_input_accessibility::insert_text_at_cursor;

    // 権限がない場合はエラーになることを確認
    match insert_text_at_cursor("test").await {
        Ok(()) => {
            println!("Text inserted successfully");
        }
        Err(TextInputError::PermissionDenied) => {
            println!("Permission denied - expected if accessibility not granted");
        }
        Err(TextInputError::NoFocusedElement) => {
            println!("No focused element - expected if no text field is focused");
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
#[ignore] // 手動実行用: フォーカス要素の検出テスト
fn test_check_focused_element() {
    println!("\n=== Focus Element Detection Test ===");
    println!("Please focus on different UI elements to test detection:");
    
    match check_focused_element_is_text_field() {
        Ok(true) => {
            println!("✅ A text field is currently focused");
        }
        Ok(false) => {
            println!("❌ No text field is focused (or focused element is not a text field)");
        }
        Err(TextInputError::PermissionDenied) => {
            println!("❌ Accessibility permission denied");
            println!("   Please grant permission in System Settings > Privacy & Security > Accessibility");
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
#[ignore] // 手動実行用: 各種アプリケーションでのテスト
fn test_multiple_applications() {
    use std::thread;
    use std::time::Duration;

    println!("\n=== Multi-Application Test ===");
    println!("This test will check focused elements every 2 seconds for 10 iterations.");
    println!("Please focus on different text fields in various applications:");
    println!("- Chrome (address bar, search fields)");
    println!("- VS Code (editor)");
    println!("- Terminal");
    println!("- Safari");
    println!("- Notes.app");
    println!("- Non-text elements (buttons, labels) to test rejection\n");

    for i in 1..=10 {
        thread::sleep(Duration::from_secs(2));
        
        print!("Check #{}: ", i);
        match check_focused_element_is_text_field() {
            Ok(true) => println!("✅ Text field detected"),
            Ok(false) => println!("❌ Not a text field"),
            Err(e) => println!("❌ Error: {}", e),
        }
    }
}

#[tokio::test]
#[cfg_attr(feature = "ci-test", ignore)]
#[ignore] // 手動実行用: テキスト挿入の完全テスト
async fn test_text_insertion_complete() {
    use voice_input::infrastructure::external::text_input_accessibility::{
        insert_text_at_cursor, check_accessibility_permission, check_focused_element_is_text_field
    };
    use std::time::Duration;
    use tokio::time::sleep;
    
    println!("\n=== Text Insertion Test ===");
    
    // 1. 権限チェック
    match check_accessibility_permission() {
        Ok(()) => println!("✅ Accessibility permission granted"),
        Err(e) => {
            println!("❌ Permission error: {}", e);
            return;
        }
    }
    
    // 2. テキストフィールドにフォーカスするよう促す
    println!("\nPlease click on a text field within 5 seconds...");
    for i in (1..=5).rev() {
        println!("  Starting in {}...", i);
        sleep(Duration::from_secs(1)).await;
    }
    
    // 3. フォーカス要素の確認
    match check_focused_element_is_text_field() {
        Ok(true) => println!("✅ Text field is focused"),
        Ok(false) => {
            println!("❌ No text field focused");
            return;
        }
        Err(e) => {
            println!("❌ Error checking focus: {}", e);
            return;
        }
    }
    
    // 4. 各種テキストの挿入テスト
    let test_cases = vec![
        ("Hello, World! ", "ASCII text"),
        ("Testing 123... ", "Alphanumeric"),
        ("こんにちは世界！ ", "Japanese text"),
        ("🚀✨🎉 ", "Emojis"),
        ("Mixed: ABC あいう 123 🎯 ", "Mixed content"),
    ];
    
    for (text, description) in test_cases {
        println!("\nTesting {}: \"{}\"", description, text);
        match insert_text_at_cursor(text).await {
            Ok(()) => println!("  ✅ Successfully inserted"),
            Err(e) => println!("  ❌ Failed: {}", e),
        }
        sleep(Duration::from_secs(1)).await;
    }
    
    println!("\n✅ Test completed. Please verify the text was inserted correctly.");
}