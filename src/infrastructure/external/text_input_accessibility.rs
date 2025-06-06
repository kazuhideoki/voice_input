//! macOS Accessibility API を使用したテキスト入力実装
//!
//! CGEventTapとの競合を避けるため、キーボードイベントを生成せず
//! 直接テキストフィールドにテキストを挿入する

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation_sys::base::{CFRelease, CFTypeRef};
use std::error::Error;
use std::fmt;

// Import our sys module
use super::accessibility_sys::*;

/// テキスト入力エラー型（統一）
#[derive(Debug)]
pub enum TextInputError {
    /// フォーカス中の要素が見つからない
    NoFocusedElement,
    /// テキストフィールドではない
    NotTextElement,
    /// API呼び出し失敗
    ApiCallFailed(String),
    /// 権限不足
    PermissionDenied,
    /// カーソル位置の取得失敗
    CursorPositionError(String),
}

impl fmt::Display for TextInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextInputError::NoFocusedElement => {
                write!(f, "No focused element found")
            }
            TextInputError::NotTextElement => {
                write!(f, "Focused element is not a text field")
            }
            TextInputError::ApiCallFailed(msg) => {
                write!(f, "Accessibility API call failed: {}", msg)
            }
            TextInputError::PermissionDenied => {
                write!(
                    f,
                    "Accessibility permission denied. Please grant accessibility access in System Settings."
                )
            }
            TextInputError::CursorPositionError(msg) => {
                write!(f, "Failed to get cursor position: {}", msg)
            }
        }
    }
}

impl Error for TextInputError {}

// Accessibility API 属性名の定義
// CFStringは直接作成するため、スレッドセーフな定数として定義できない
fn create_ax_focused_ui_element_attribute() -> CFString {
    CFString::from_static_string("AXFocusedUIElement")
}

fn create_ax_value_attribute() -> CFString {
    CFString::from_static_string("AXValue")
}

fn create_ax_role_attribute() -> CFString {
    CFString::from_static_string("AXRole")
}

fn create_ax_selected_text_range_attribute() -> CFString {
    CFString::from_static_string("AXSelectedTextRange")
}

// テキスト入力可能なRole
fn create_ax_text_area_role() -> CFString {
    CFString::from_static_string("AXTextArea")
}

fn create_ax_text_field_role() -> CFString {
    CFString::from_static_string("AXTextField")
}

fn create_ax_combo_box_role() -> CFString {
    CFString::from_static_string("AXComboBox")
}

fn create_ax_search_field_role() -> CFString {
    CFString::from_static_string("AXSearchField")
}

/// CFStringの比較ヘルパー
fn cfstring_equals(s1: CFStringRef, s2: &CFString) -> bool {
    unsafe {
        let s1_str = CFString::wrap_under_get_rule(s1).to_string();
        let s2_str = s2.to_string();
        s1_str == s2_str
    }
}

/// CFRangeからカーソル位置を抽出
fn extract_cursor_position_from_range(range_value: CFTypeRef) -> Result<usize, TextInputError> {
    // AXSelectedTextRangeはCFRangeを返す（location, length）
    // CFRangeは2つのCFIndexを含む構造体
    #[repr(C)]
    struct CFRange {
        location: core_foundation_sys::base::CFIndex,
        length: core_foundation_sys::base::CFIndex,
    }

    unsafe {
        // CFTypeRefをCFRangeポインタとして扱う
        let range_ptr = range_value as *const CFRange;
        if range_ptr.is_null() {
            return Err(TextInputError::CursorPositionError(
                "Null range pointer".to_string(),
            ));
        }

        let range = &*range_ptr;

        // locationが負の値の場合はエラー
        if range.location < 0 {
            return Err(TextInputError::CursorPositionError(format!(
                "Invalid cursor position: {}",
                range.location
            )));
        }

        Ok(range.location as usize)
    }
}

/// 権限チェックと要求
pub fn check_accessibility_permission() -> Result<(), TextInputError> {
    unsafe {
        if AXIsProcessTrusted() != 0 {
            Ok(())
        } else {
            // 権限ダイアログを表示するオプション
            let prompt_key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
            let cf_true = core_foundation::boolean::CFBoolean::true_value();

            let options = core_foundation::dictionary::CFDictionary::from_CFType_pairs(&[(
                prompt_key.as_CFType(),
                cf_true.as_CFType(),
            )]);

            AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as CFTypeRef);
            Err(TextInputError::PermissionDenied)
        }
    }
}

/// フォーカス中のテキストフィールドに直接テキストを挿入
///
/// カーソル位置に挿入（既存テキストを保持）
pub async fn insert_text_at_cursor(text: &str) -> Result<(), TextInputError> {
    tokio::task::spawn_blocking({
        let text = text.to_string();
        move || insert_text_sync(&text)
    })
    .await
    .map_err(|e| TextInputError::ApiCallFailed(e.to_string()))?
}

/// デバッグ用：フォーカス要素の情報を取得
pub fn debug_focused_element() -> Result<String, TextInputError> {
    check_accessibility_permission()?;

    unsafe {
        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            return Err(TextInputError::ApiCallFailed(
                "Failed to get system-wide element".to_string(),
            ));
        }

        let mut focused_element: CFTypeRef = std::ptr::null_mut();
        let attr_name = create_ax_focused_ui_element_attribute();
        let result = AXUIElementCopyAttributeValue(
            system_wide,
            attr_name.as_concrete_TypeRef(),
            &mut focused_element,
        );

        CFRelease(system_wide as CFTypeRef);

        if result != kAXErrorSuccess || focused_element.is_null() {
            return Err(TextInputError::NoFocusedElement);
        }

        // Get role
        let mut role_value: CFTypeRef = std::ptr::null_mut();
        let role_attr = create_ax_role_attribute();
        let role_result = AXUIElementCopyAttributeValue(
            focused_element as AXUIElementRef,
            role_attr.as_concrete_TypeRef(),
            &mut role_value,
        );

        let role = if role_result == kAXErrorSuccess && !role_value.is_null() {
            let cf_string = CFString::wrap_under_get_rule(role_value as CFStringRef);
            let role_str = cf_string.to_string();
            CFRelease(role_value);
            role_str
        } else {
            "Unknown".to_string()
        };

        CFRelease(focused_element);

        Ok(format!("Focused element role: {}", role))
    }
}

/// フォーカス中の要素がテキストフィールドかチェック（デバッグ/テスト用）
pub fn check_focused_element_is_text_field() -> Result<bool, TextInputError> {
    // 権限チェック
    check_accessibility_permission()?;

    // フォーカス要素を取得
    let element = match get_focused_element() {
        Ok(elem) => elem,
        Err(TextInputError::NoFocusedElement) => return Ok(false),
        Err(e) => return Err(e),
    };

    // テキストフィールドかチェック
    let result = validate_text_element(&element);

    // 要素をリリース
    unsafe {
        CFRelease(element as CFTypeRef);
    }

    match result {
        Ok(()) => Ok(true),
        Err(TextInputError::NotTextElement) => Ok(false),
        Err(e) => Err(e),
    }
}

/// 同期版テキスト挿入（内部実装）
fn insert_text_sync(text: &str) -> Result<(), TextInputError> {
    // 1. 権限チェック
    check_accessibility_permission()?;

    // 2. システム全体のフォーカス中要素を取得
    let focused_element = get_focused_element()?;

    // 3. テキストフィールドかどうか確認
    let validation_result = validate_text_element(&focused_element);
    if validation_result.is_err() {
        unsafe {
            CFRelease(focused_element as CFTypeRef);
        }
        return validation_result;
    }

    // 4. カーソル位置に挿入
    let insert_result = insert_at_cursor_position(&focused_element, text);

    // 5. 要素をリリース
    unsafe {
        CFRelease(focused_element as CFTypeRef);
    }

    insert_result
}

/// フォーカス中の要素を取得
fn get_focused_element() -> Result<AXUIElementRef, TextInputError> {
    unsafe {
        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            return Err(TextInputError::ApiCallFailed(
                "Failed to get system-wide element".to_string(),
            ));
        }

        let mut focused_element: CFTypeRef = std::ptr::null_mut();

        let attr_name = create_ax_focused_ui_element_attribute();
        let result = AXUIElementCopyAttributeValue(
            system_wide,
            attr_name.as_concrete_TypeRef(),
            &mut focused_element,
        );

        // system_wideのリリース
        CFRelease(system_wide as CFTypeRef);

        match result {
            r if r == kAXErrorSuccess && !focused_element.is_null() => {
                Ok(focused_element as AXUIElementRef)
            }
            r if r == kAXErrorAPIDisabled => Err(TextInputError::PermissionDenied),
            r if r == kAXErrorNoValue => Err(TextInputError::NoFocusedElement),
            _ => Err(TextInputError::NoFocusedElement),
        }
    }
}

/// テキストフィールドかどうか確認
fn validate_text_element(element: &AXUIElementRef) -> Result<(), TextInputError> {
    unsafe {
        let mut role_value: CFTypeRef = std::ptr::null_mut();

        let role_attr = create_ax_role_attribute();
        let result = AXUIElementCopyAttributeValue(
            *element,
            role_attr.as_concrete_TypeRef(),
            &mut role_value,
        );

        if result != kAXErrorSuccess || role_value.is_null() {
            return Err(TextInputError::ApiCallFailed(
                "Failed to get element role".to_string(),
            ));
        }

        // roleをCFStringとして扱う
        let role_string = role_value as CFStringRef;

        // テキスト入力可能なroleかチェック
        let is_text_element = cfstring_equals(role_string, &create_ax_text_field_role())
            || cfstring_equals(role_string, &create_ax_text_area_role())
            || cfstring_equals(role_string, &create_ax_combo_box_role())
            || cfstring_equals(role_string, &create_ax_search_field_role());

        // roleのリリース
        CFRelease(role_value);

        if is_text_element {
            // さらに編集可能かチェック（一部のテキストフィールドは読み取り専用の場合がある）
            let mut value: CFTypeRef = std::ptr::null_mut();
            let value_attr = create_ax_value_attribute();
            let value_result = AXUIElementCopyAttributeValue(
                *element,
                value_attr.as_concrete_TypeRef(),
                &mut value,
            );

            if value_result == kAXErrorSuccess && !value.is_null() {
                CFRelease(value);
                Ok(())
            } else if value_result == kAXErrorAttributeUnsupported {
                // Value属性がサポートされていない場合はテキストフィールドではない
                Err(TextInputError::NotTextElement)
            } else {
                // その他のエラーも含めて、安全のためOKとする（挿入時にエラーになる）
                Ok(())
            }
        } else {
            Err(TextInputError::NotTextElement)
        }
    }
}

/// カーソル位置にテキストを挿入
fn insert_at_cursor_position(element: &AXUIElementRef, text: &str) -> Result<(), TextInputError> {
    unsafe {
        // 1. 既存のテキストを取得
        let mut current_value: CFTypeRef = std::ptr::null_mut();
        let value_attr = create_ax_value_attribute();
        let value_result = AXUIElementCopyAttributeValue(
            *element,
            value_attr.as_concrete_TypeRef(),
            &mut current_value,
        );

        let current_text = if value_result == kAXErrorSuccess && !current_value.is_null() {
            // CFStringとして扱い、文字列に変換
            let cf_string = CFString::wrap_under_get_rule(current_value as CFStringRef);
            let text = cf_string.to_string();
            CFRelease(current_value);
            text
        } else {
            String::new()
        };

        // 2. カーソル位置を取得
        let mut range_value: CFTypeRef = std::ptr::null_mut();
        let range_attr = create_ax_selected_text_range_attribute();
        let range_result = AXUIElementCopyAttributeValue(
            *element,
            range_attr.as_concrete_TypeRef(),
            &mut range_value,
        );

        let cursor_position = if range_result == kAXErrorSuccess && !range_value.is_null() {
            // CFRangeからカーソル位置を取得
            let cursor_pos = extract_cursor_position_from_range(range_value)?;
            CFRelease(range_value);
            cursor_pos
        } else {
            // カーソル位置が取得できない場合は末尾に追加
            current_text.len()
        };

        // 3. 新しいテキストを構築（カーソル位置に挿入）
        let mut new_text = String::with_capacity(current_text.len() + text.len());

        // UTF-8のバイト位置を正しく扱う
        let char_indices: Vec<(usize, char)> = current_text.char_indices().collect();
        let byte_position = if cursor_position >= char_indices.len() {
            current_text.len()
        } else {
            char_indices[cursor_position].0
        };

        new_text.push_str(&current_text[..byte_position]);
        new_text.push_str(text);
        new_text.push_str(&current_text[byte_position..]);

        // 4. 新しいテキストを設定
        let new_value = CFString::new(&new_text);
        let result = AXUIElementSetAttributeValue(
            *element,
            value_attr.as_concrete_TypeRef(),
            new_value.as_concrete_TypeRef() as CFTypeRef,
        );

        // 要素をリリース（insert_text_syncで行うので、ここでは不要）

        if result == kAXErrorSuccess {
            Ok(())
        } else {
            Err(TextInputError::ApiCallFailed(format!(
                "Failed to set value: error code {}",
                result
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = TextInputError::PermissionDenied;
        let message = format!("{}", error);
        assert!(message.contains("System Settings"));
    }

    #[test]
    fn test_ax_error_values() {
        assert_eq!(kAXErrorSuccess, 0);
        assert_eq!(kAXErrorFailure, -25200);
    }

    // 実際のAPI呼び出しテストは手動テストで行う
    #[test]
    #[ignore] // 手動実行用
    fn test_check_accessibility_permission() {
        match check_accessibility_permission() {
            Ok(()) => println!("Accessibility permission granted"),
            Err(e) => println!("Permission error: {}", e),
        }
    }

    #[test]
    #[ignore] // 手動実行用
    fn test_focused_element() {
        match check_accessibility_permission() {
            Ok(()) => println!("Permission OK"),
            Err(e) => {
                println!("Permission error: {}", e);
                return;
            }
        }

        // Test getting focused element
        match get_focused_element() {
            Ok(element) => {
                println!("Got focused element: {:?}", element);

                // Test validation
                match validate_text_element(&element) {
                    Ok(()) => println!("Element is a text field"),
                    Err(e) => println!("Not a text field: {}", e),
                }

                unsafe {
                    CFRelease(element as CFTypeRef);
                }
            }
            Err(e) => println!("Failed to get focused element: {}", e),
        }
    }

    #[tokio::test]
    #[ignore] // 手動実行用
    async fn test_text_insertion() {
        // Simple test for text insertion
        match insert_text_at_cursor("Test text 123").await {
            Ok(()) => println!("Text inserted successfully"),
            Err(e) => println!("Failed to insert text: {}", e),
        }
    }
}
