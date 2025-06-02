//! 権限管理モジュール - macOS権限システムの統一インターフェース
//! 
//! # 概要
//! macOS権限システムの複雑性を抽象化し、将来的な拡張性を提供します。
//! Phase 2では`AccessibilityPermissions`のみを実装しますが、
//! 将来的にはInput Monitoring等の追加権限にも対応可能な設計です。

pub use accessibility::AccessibilityPermissions;

mod accessibility;

/// 権限状態を表す列挙型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionStatus {
    /// 権限が付与されている
    Granted,
    /// 権限が拒否されている
    Denied,
    /// 権限状態が未確定（初回要求前）
    NotDetermined,
}

/// 権限チェック・要求の統一インターフェース
pub trait PermissionChecker {
    /// 権限状態を確認
    fn check_status() -> PermissionStatus;
    
    /// システム環境設定を開く
    fn open_system_preferences() -> Result<(), String>;
    
    /// ユーザー向けエラーメッセージを取得
    fn get_error_message() -> String;
    
    /// 権限要求の理由説明を取得
    fn get_permission_description() -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_status_equality() {
        assert_eq!(PermissionStatus::Granted, PermissionStatus::Granted);
        assert_ne!(PermissionStatus::Granted, PermissionStatus::Denied);
        assert_ne!(PermissionStatus::Denied, PermissionStatus::NotDetermined);
    }

    #[test]
    fn test_permission_status_debug() {
        let status = PermissionStatus::Granted;
        assert_eq!(format!("{:?}", status), "Granted");
    }
}