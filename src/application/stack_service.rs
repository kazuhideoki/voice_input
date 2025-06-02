use crate::domain::stack::{Stack, StackInfo};
use crate::infrastructure::ui::{StackDisplayInfo, UiNotification};
use std::collections::HashMap;
use std::fmt;
use std::sync::Weak;
use std::time::SystemTime;

/// スタック管理エラー型
#[derive(Debug, Clone)]
pub enum StackServiceError {
    /// 指定されたスタックが見つからない (requested_id, available_ids)
    StackNotFound(u32, Vec<u32>),
    /// スタックモードが無効
    StackModeDisabled,
    /// テキストが大きすぎる (text_size)
    TextTooLarge(usize),
}

impl fmt::Display for StackServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StackServiceError::StackNotFound(id, available) => {
                if available.is_empty() {
                    write!(
                        f,
                        "❌ Stack {} not found. No stacks saved. Use 'voice_input start' to create stacks.",
                        id
                    )
                } else {
                    let available_str = available
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    write!(
                        f,
                        "❌ Stack {} not found. Available stacks: {}",
                        id, available_str
                    )
                }
            }
            StackServiceError::StackModeDisabled => {
                write!(
                    f,
                    "❌ Stack mode is not enabled. Run 'voice_input stack-mode on' first."
                )
            }
            StackServiceError::TextTooLarge(size) => {
                write!(
                    f,
                    "❌ Text too large ({} characters). Maximum size is {} characters.",
                    size,
                    StackService::MAX_STACK_SIZE
                )
            }
        }
    }
}

impl std::error::Error for StackServiceError {}

/// UI通知ハンドラーのトレイト
pub trait UiNotificationHandler: Send + Sync {
    fn notify(&self, notification: UiNotification) -> Result<(), String>;
}

/// スタック管理サービス
///
/// **重要**: 完全にオンメモリ管理。スタックモード無効化またはデーモン再起動時に全データ消失。
pub struct StackService {
    /// スタックモードが有効かどうか
    mode_enabled: bool,
    /// スタック保存用（番号 -> Stack）**オンメモリのみ**
    stacks: HashMap<u32, Stack>,
    /// 次に割り当てるスタック番号
    next_id: u32,
    /// UI通知ハンドラー（オプショナル）
    ui_handler: Option<Weak<dyn UiNotificationHandler>>,
}

impl Default for StackService {
    fn default() -> Self {
        Self::new()
    }
}

impl StackService {
    /// 最大スタック数（メモリ保護）
    pub const MAX_STACKS: usize = 50;
    /// 最大スタックサイズ（大容量テキスト制限）
    pub const MAX_STACK_SIZE: usize = 10_000;
    /// プレビュー長さ
    pub const PREVIEW_LENGTH: usize = 40;

    pub fn new() -> Self {
        Self {
            mode_enabled: false,
            stacks: HashMap::new(),
            next_id: 1,
            ui_handler: None,
        }
    }

    /// UI通知ハンドラーを設定
    pub fn set_ui_handler(&mut self, handler: Weak<dyn UiNotificationHandler>) {
        self.ui_handler = Some(handler);
    }

    /// UI通知を送信
    fn notify_ui(&self, notification: UiNotification) {
        if let Some(handler_weak) = &self.ui_handler {
            if let Some(handler) = handler_weak.upgrade() {
                let _ = handler.notify(notification);
            }
        }
    }

    /// StackをStackDisplayInfoに変換
    fn stack_to_display_info(&self, stack: &Stack, is_active: bool) -> StackDisplayInfo {
        let preview = if stack.text.chars().count() > Self::PREVIEW_LENGTH {
            let truncated: String = stack.text.chars().take(Self::PREVIEW_LENGTH).collect();
            format!("{}...", truncated)
        } else {
            stack.text.clone()
        };

        // SystemTimeを簡易的にフォーマット
        let created_at =
            if let Ok(duration) = stack.created_at.duration_since(SystemTime::UNIX_EPOCH) {
                let secs = duration.as_secs();
                let hours = (secs / 3600) % 24;
                let minutes = (secs / 60) % 60;
                let seconds = secs % 60;
                format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
            } else {
                "00:00:00".to_string()
            };

        StackDisplayInfo {
            number: stack.id,
            preview,
            created_at,
            is_active,
            char_count: stack.text.len(),
        }
    }

    /// スタックモードが有効かどうか
    pub fn is_stack_mode_enabled(&self) -> bool {
        self.mode_enabled
    }

    /// スタックモードを有効化
    pub fn enable_stack_mode(&mut self) -> bool {
        self.mode_enabled = true;
        self.notify_ui(UiNotification::ModeChanged(true));
        true
    }

    /// スタックモードを無効化
    pub fn disable_stack_mode(&mut self) -> bool {
        self.mode_enabled = false;
        self.stacks.clear();
        self.next_id = 1;
        self.notify_ui(UiNotification::ModeChanged(false));
        true
    }

    /// 新しいスタックを保存
    pub fn save_stack(&mut self, text: String) -> u32 {
        let id = self.next_id;
        let stack = Stack::new(id, text);
        let display_info = self.stack_to_display_info(&stack, false);
        self.stacks.insert(id, stack);
        self.next_id += 1;

        self.notify_ui(UiNotification::StackAdded(display_info));
        id
    }

    /// 最適化されたスタック保存（サイズチェック付き）
    pub fn save_stack_optimized(&mut self, text: String) -> Result<u32, StackServiceError> {
        // サイズチェック
        if text.len() > Self::MAX_STACK_SIZE {
            return Err(StackServiceError::TextTooLarge(text.len()));
        }

        // 容量チェック・自動削除
        if self.stacks.len() >= Self::MAX_STACKS {
            self.remove_oldest_stack();
        }

        let id = self.next_id;
        let stack = Stack::new(id, text);
        let display_info = self.stack_to_display_info(&stack, false);
        self.stacks.insert(id, stack);
        self.next_id += 1;

        self.notify_ui(UiNotification::StackAdded(display_info));
        Ok(id)
    }

    /// 最古のスタックを削除
    fn remove_oldest_stack(&mut self) {
        if let Some(&oldest_id) = self.stacks.keys().min() {
            self.stacks.remove(&oldest_id);
        }
    }

    /// 指定番号のスタックを取得
    pub fn get_stack(&self, number: u32) -> Option<&Stack> {
        self.stacks.get(&number)
    }

    /// 指定番号のスタックを取得（エラーコンテキスト付き）
    pub fn get_stack_with_context(&self, number: u32) -> Result<&Stack, StackServiceError> {
        if !self.mode_enabled {
            return Err(StackServiceError::StackModeDisabled);
        }

        match self.stacks.get(&number) {
            Some(stack) => {
                self.notify_ui(UiNotification::StackAccessed(number));
                Ok(stack)
            }
            None => {
                let available: Vec<u32> = self.stacks.keys().cloned().collect();
                Err(StackServiceError::StackNotFound(number, available))
            }
        }
    }

    /// 全スタックの情報を取得
    pub fn list_stacks(&self) -> Vec<StackInfo> {
        let mut infos: Vec<_> = self.stacks.values().map(|stack| stack.to_info()).collect();
        infos.sort_by_key(|info| info.number);
        infos
    }

    /// 全スタックをクリア
    pub fn clear_stacks(&mut self) {
        self.stacks.clear();
        self.next_id = 1;
        self.notify_ui(UiNotification::StacksCleared);
    }

    /// 確認メッセージ付きクリア
    pub fn clear_stacks_with_confirmation(&mut self) -> (usize, String) {
        let count = self.stacks.len();
        self.clear_stacks();

        let message = if count > 0 {
            format!("✅ Cleared {} stack(s) from memory.", count)
        } else {
            "📝 No stacks to clear.".to_string()
        };

        (count, message)
    }

    /// フォーマット済み一覧表示
    pub fn list_stacks_formatted(&self) -> String {
        if self.stacks.is_empty() {
            return "📝 No stacks saved. Use 'voice_input start' to create stacks.".to_string();
        }

        let mut output = format!("📚 {} stack(s) in memory:\n", self.stacks.len());

        for info in self.list_stacks() {
            output.push_str(&format!(
                "  [{}] {} ({})\n",
                info.number, info.preview, info.created_at
            ));
        }

        output.push_str("\n💡 Use 'voice_input paste <number>' to paste any stack.");
        output
    }

    /// ショートカット機能が有効化されたことを通知
    /// Phase 2で追加: ショートカット連携インターフェース
    pub fn notify_shortcut_enabled(&mut self) -> Result<(), String> {
        if !self.mode_enabled {
            return Err("Stack mode is not enabled".to_string());
        }

        println!("📍 Shortcut functionality enabled for stack mode");
        self.notify_ui(UiNotification::ModeChanged(true));
        Ok(())
    }

    /// ショートカット機能が無効化されたことを通知
    /// Phase 2で追加: ショートカット連携インターフェース
    pub fn notify_shortcut_disabled(&mut self) -> Result<(), String> {
        println!("📍 Shortcut functionality disabled");
        // ショートカット無効化はスタックモード自体には影響しない
        Ok(())
    }

    /// 指定番号のショートカットペースト対象テキストを取得
    /// Phase 2で追加: ショートカット連携インターフェース
    pub fn get_shortcut_paste_target(&self, number: u32) -> Option<String> {
        if !self.mode_enabled {
            return None;
        }

        self.stacks.get(&number).map(|stack| stack.text.clone())
    }

    /// ショートカット統合の整合性を検証
    /// Phase 2で追加: ショートカット連携インターフェース
    pub fn validate_shortcut_integration(&self) -> bool {
        // スタックモードが有効で、スタックが存在する場合に統合が有効
        self.mode_enabled && !self.stacks.is_empty()
    }
}

/// ユーザーフィードバック
pub struct UserFeedback;

impl UserFeedback {
    pub fn stack_saved(id: u32, preview: &str) -> String {
        format!("📝 Stack {} saved: {}", id, preview)
    }

    pub fn paste_success(id: u32, chars: usize) -> String {
        format!("✅ Pasted stack {} ({} characters)", id, chars)
    }

    pub fn stack_not_found(id: u32, available: &[u32]) -> String {
        let list = available
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("❌ Stack {} not found. Available: [{}]", id, list)
    }

    pub fn mode_status(enabled: bool, count: usize) -> String {
        if enabled {
            format!("🟢 Stack mode ON ({} stacks in memory)", count)
        } else {
            "🔴 Stack mode OFF".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_service_creation() {
        let service = StackService::new();
        assert!(!service.is_stack_mode_enabled());
    }

    #[test]
    fn test_enable_disable_stack_mode() {
        let mut service = StackService::new();
        assert!(service.enable_stack_mode());
        assert!(service.is_stack_mode_enabled());
        assert!(service.disable_stack_mode());
        assert!(!service.is_stack_mode_enabled());
    }

    #[test]
    fn test_save_and_get_stack() {
        let mut service = StackService::new();
        let id = service.save_stack("Test text".to_string());
        assert_eq!(id, 1);

        let stack = service.get_stack(1).unwrap();
        assert_eq!(stack.text, "Test text");
        assert_eq!(stack.id, 1);
    }

    #[test]
    fn test_list_and_clear_stacks() {
        let mut service = StackService::new();
        service.save_stack("First".to_string());
        service.save_stack("Second".to_string());

        let list = service.list_stacks();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].number, 1);
        assert_eq!(list[1].number, 2);

        service.clear_stacks();
        assert_eq!(service.list_stacks().len(), 0);
    }

    #[test]
    fn test_shortcut_integration_methods() {
        let mut service = StackService::new();
        
        // スタックモード無効時のテスト
        assert!(service.notify_shortcut_enabled().is_err());
        assert_eq!(service.get_shortcut_paste_target(1), None);
        assert!(!service.validate_shortcut_integration());
        
        // スタックモード有効化
        service.enable_stack_mode();
        
        // スタック無し状態
        assert!(service.notify_shortcut_enabled().is_ok());
        assert!(!service.validate_shortcut_integration());
        
        // スタック追加後
        service.save_stack("Test content".to_string());
        assert!(service.validate_shortcut_integration());
        assert_eq!(service.get_shortcut_paste_target(1), Some("Test content".to_string()));
        assert_eq!(service.get_shortcut_paste_target(999), None);
        
        // ショートカット無効化テスト
        assert!(service.notify_shortcut_disabled().is_ok());
    }

    #[test]
    fn test_shortcut_paste_target_retrieval() {
        let mut service = StackService::new();
        service.enable_stack_mode();
        
        // 複数スタック追加
        service.save_stack("First stack content".to_string());
        service.save_stack("Second stack content".to_string());
        service.save_stack("Third stack content".to_string());
        
        // 各スタックの取得確認
        assert_eq!(service.get_shortcut_paste_target(1), Some("First stack content".to_string()));
        assert_eq!(service.get_shortcut_paste_target(2), Some("Second stack content".to_string()));
        assert_eq!(service.get_shortcut_paste_target(3), Some("Third stack content".to_string()));
        
        // 存在しないスタック
        assert_eq!(service.get_shortcut_paste_target(4), None);
        assert_eq!(service.get_shortcut_paste_target(0), None);
    }

    #[test]
    fn test_shortcut_integration_with_mode_changes() {
        let mut service = StackService::new();
        
        // スタック追加してからモード有効化
        service.save_stack("Test".to_string());
        assert!(!service.validate_shortcut_integration()); // モード無効
        assert_eq!(service.get_shortcut_paste_target(1), None); // モード無効
        
        // モード有効化
        service.enable_stack_mode();
        assert!(service.validate_shortcut_integration()); // モード有効+スタックあり
        assert!(service.get_shortcut_paste_target(1).is_some()); // モード有効
        
        // スタッククリア
        service.clear_stacks();
        assert!(!service.validate_shortcut_integration()); // モード有効だがスタック無し
        
        // モード無効化
        service.disable_stack_mode();
        service.save_stack("Another test".to_string());
        assert!(!service.validate_shortcut_integration()); // モード無効
    }
}
