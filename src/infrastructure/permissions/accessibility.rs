//! macOSアクセシビリティ権限チェック専用実装
//! 
//! # 概要
//! CoreFoundation FFIを使用してアクセシビリティ権限の状態確認と
//! システム環境設定への誘導を行います。

use super::{PermissionChecker, PermissionStatus};

/// アクセシビリティ権限管理構造体
pub struct AccessibilityPermissions;

impl AccessibilityPermissions {
    /// 簡易権限チェック（互換性維持用）
    /// Phase 1の`ShortcutService::check_accessibility_permission()`の代替
    pub fn check() -> bool {
        matches!(Self::check_status(), PermissionStatus::Granted)
    }
}

impl PermissionChecker for AccessibilityPermissions {
    /// アクセシビリティ権限状態の詳細確認
    fn check_status() -> PermissionStatus {
        // CI環境では常にGrantedを返してテストを継続可能にする
        #[cfg(feature = "ci-test")]
        {
            return PermissionStatus::Granted;
        }

        // 実際のmacOS権限チェック
        #[cfg(not(feature = "ci-test"))]
        {
            use core_foundation::base::CFTypeRef;
            use std::ptr;
            
            unsafe {
                let trusted = ffi::AXIsProcessTrusted();
                if trusted {
                    PermissionStatus::Granted
                } else {
                    // 権限要求を行って状態を再確認
                    ffi::AXIsProcessTrustedWithOptions(ptr::null() as CFTypeRef);
                    let trusted_after_request = ffi::AXIsProcessTrusted();
                    if trusted_after_request {
                        PermissionStatus::Granted
                    } else {
                        PermissionStatus::Denied
                    }
                }
            }
        }
    }

    /// システム環境設定のアクセシビリティ画面を開く
    fn open_system_preferences() -> Result<(), String> {
        #[cfg(feature = "ci-test")]
        {
            // CI環境ではシミュレート
            println!("CI Mode: Would open System Preferences > Security & Privacy > Accessibility");
            return Ok(());
        }

        #[cfg(not(feature = "ci-test"))]
        {
            use std::process::Command;
            
            // macOS 13以降の新しいシステム設定
            let result = Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                .output();

            match result {
                Ok(output) if output.status.success() => Ok(()),
                Ok(_) => {
                    // フォールバック: 古いシステム環境設定
                    let fallback = Command::new("open")
                        .arg("/System/Library/PreferencePanes/Security.prefPane")
                        .output();
                    
                    match fallback {
                        Ok(fb_output) if fb_output.status.success() => Ok(()),
                        _ => Err("Failed to open System Preferences".to_string()),
                    }
                }
                Err(e) => Err(format!("Failed to execute open command: {}", e)),
            }
        }
    }

    /// ユーザー向けエラーメッセージ（日本語）
    fn get_error_message() -> String {
        "❌ アクセシビリティ権限が必要です。\n\
         システム環境設定 → セキュリティとプライバシー → プライバシー → アクセシビリティ\n\
         で本アプリケーションに権限を付与してください。".to_string()
    }

    /// 権限要求の理由説明
    fn get_permission_description() -> String {
        "Voice Inputでキーボードショートカット機能を使用するために、\n\
         アクセシビリティ権限が必要です。\n\
         この権限により、Cmd+Rでの録音開始/停止、Cmd+数字でのスタックペーストが可能になります。".to_string()
    }
}

/// CoreFoundation FFI wrapper（内部実装）
mod ffi {
    use core_foundation::base::CFTypeRef;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        /// アクセシビリティ権限が信頼されているかチェック
        pub fn AXIsProcessTrusted() -> bool;
        
        /// アクセシビリティ権限要求付きチェック
        /// options: kAXTrustedCheckOptionPrompt を含むCFDictionary
        pub fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessibility_permissions_simple_check() {
        // CI環境では常にtrue、実環境では実際の権限状態
        let result = AccessibilityPermissions::check();
        
        #[cfg(feature = "ci-test")]
        assert!(result, "CI mode should always return true");
        
        #[cfg(not(feature = "ci-test"))]
        {
            // 実環境では結果は権限状態次第
            println!("Accessibility permission status: {}", result);
        }
    }

    #[test]
    fn test_permission_status_check() {
        let status = AccessibilityPermissions::check_status();
        
        #[cfg(feature = "ci-test")]
        assert_eq!(status, PermissionStatus::Granted, "CI mode should return Granted");
        
        #[cfg(not(feature = "ci-test"))]
        {
            // 実環境では結果は実際の権限状態次第
            println!("Detailed permission status: {:?}", status);
        }
    }

    #[test]
    fn test_error_message_format() {
        let message = AccessibilityPermissions::get_error_message();
        assert!(message.contains("アクセシビリティ権限"));
        assert!(message.contains("システム環境設定"));
    }

    #[test]
    fn test_permission_description() {
        let description = AccessibilityPermissions::get_permission_description();
        assert!(description.contains("Voice Input"));
        assert!(description.contains("キーボードショートカット"));
        assert!(description.contains("Cmd+R"));
    }

    #[test]
    fn test_open_system_preferences() {
        // CI環境では常に成功
        #[cfg(feature = "ci-test")]
        {
            let result = AccessibilityPermissions::open_system_preferences();
            assert!(result.is_ok(), "CI mode should simulate successful opening");
        }

        // 実環境ではmacOSが必要
        #[cfg(not(feature = "ci-test"))]
        {
            let result = AccessibilityPermissions::open_system_preferences();
            // 実環境での結果はOS状態次第なのでテストでは確認しない
            println!("System preferences open result: {:?}", result);
        }
    }
}