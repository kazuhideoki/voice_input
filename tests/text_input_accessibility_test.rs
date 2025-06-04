//! text_input_accessibility モジュールの単体テスト

use voice_input::infrastructure::external::text_input_accessibility::{
    check_accessibility_permission, TextInputError,
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