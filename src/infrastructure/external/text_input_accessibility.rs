//! macOS Accessibility API を使用したテキスト入力実装
//! 
//! CGEventTapとの競合を避けるため、キーボードイベントを生成せず
//! 直接テキストフィールドにテキストを挿入する

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation_sys::base::CFTypeRef;
use std::error::Error;
use std::fmt;
use std::os::raw::c_void;

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
                write!(f, "Accessibility permission denied. Please grant accessibility access in System Settings.")
            }
            TextInputError::CursorPositionError(msg) => {
                write!(f, "Failed to get cursor position: {}", msg)
            }
        }
    }
}

impl Error for TextInputError {}

// AXUIElement型の定義
type AXUIElementRef = *mut c_void;

// AXError型の定義
#[repr(i32)]
#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq)]
enum AXError {
    kAXErrorSuccess = 0,
    kAXErrorFailure = -25200,
    kAXErrorIllegalArgument = -25201,
    kAXErrorInvalidUIElement = -25202,
    kAXErrorInvalidUIElementObserver = -25203,
    kAXErrorCannotComplete = -25204,
    kAXErrorAttributeUnsupported = -25205,
    kAXErrorActionUnsupported = -25206,
    kAXErrorNotificationUnsupported = -25207,
    kAXErrorNotImplemented = -25208,
    kAXErrorNotificationAlreadyRegistered = -25209,
    kAXErrorNotificationNotRegistered = -25210,
    kAXErrorAPIDisabled = -25211,
    kAXErrorNoValue = -25212,
    kAXErrorParameterizedAttributeUnsupported = -25213,
    kAXErrorNotEnoughPrecision = -25214,
}

// FFI bindings
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    fn AXUIElementCopySystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
}

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

/// 権限チェックと要求
pub fn check_accessibility_permission() -> Result<(), TextInputError> {
    unsafe {
        if AXIsProcessTrusted() {
            Ok(())
        } else {
            // 権限ダイアログを表示するオプション
            let prompt_key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
            let cf_true = core_foundation::boolean::CFBoolean::true_value();
            
            let options = core_foundation::dictionary::CFDictionary::from_CFType_pairs(&[
                (prompt_key.as_CFType(), cf_true.as_CFType())
            ]);
            
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

/// 同期版テキスト挿入（内部実装）
fn insert_text_sync(text: &str) -> Result<(), TextInputError> {
    // 1. 権限チェック
    check_accessibility_permission()?;
    
    // 2. システム全体のフォーカス中要素を取得
    let focused_element = get_focused_element()?;
    
    // 3. テキストフィールドかどうか確認
    validate_text_element(&focused_element)?;
    
    // 4. カーソル位置に挿入
    insert_at_cursor_position(&focused_element, text)?;
    
    Ok(())
}

/// フォーカス中の要素を取得
fn get_focused_element() -> Result<AXUIElementRef, TextInputError> {
    unsafe {
        let system_wide = AXUIElementCopySystemWide();
        let mut focused_element: CFTypeRef = std::ptr::null_mut();
        
        let attr_name = create_ax_focused_ui_element_attribute();
        let result = AXUIElementCopyAttributeValue(
            system_wide,
            attr_name.as_concrete_TypeRef(),
            &mut focused_element,
        );
        
        if result == AXError::kAXErrorSuccess && !focused_element.is_null() {
            Ok(focused_element as AXUIElementRef)
        } else {
            Err(TextInputError::NoFocusedElement)
        }
    }
}

/// テキストフィールドかどうか確認
fn validate_text_element(_element: &AXUIElementRef) -> Result<(), TextInputError> {
    // TODO: Task 2で実装
    // 現在は仮実装として常にOKを返す
    Ok(())
}

/// カーソル位置にテキストを挿入
fn insert_at_cursor_position(_element: &AXUIElementRef, _text: &str) -> Result<(), TextInputError> {
    // TODO: Task 3で実装
    // 現在は仮実装として常にOKを返す
    Ok(())
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
        assert_eq!(AXError::kAXErrorSuccess as i32, 0);
        assert_eq!(AXError::kAXErrorFailure as i32, -25200);
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
}