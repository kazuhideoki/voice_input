use crate::domain::stack::{Stack, StackInfo};
use std::collections::HashMap;
use std::fmt;

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

/// スタック管理サービス
///
/// **重要**: 完全にオンメモリ管理。スタックモード無効化またはデーモン再起動時に全データ消失。
#[derive(Debug, Default)]
pub struct StackService {
    /// スタックモードが有効かどうか
    mode_enabled: bool,
    /// スタック保存用（番号 -> Stack）**オンメモリのみ**
    stacks: HashMap<u32, Stack>,
    /// 次に割り当てるスタック番号
    next_id: u32,
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
        }
    }

    /// スタックモードが有効かどうか
    pub fn is_stack_mode_enabled(&self) -> bool {
        self.mode_enabled
    }

    /// スタックモードを有効化
    pub fn enable_stack_mode(&mut self) -> bool {
        self.mode_enabled = true;
        true
    }

    /// スタックモードを無効化
    pub fn disable_stack_mode(&mut self) -> bool {
        self.mode_enabled = false;
        self.stacks.clear();
        self.next_id = 1;
        true
    }

    /// 新しいスタックを保存
    pub fn save_stack(&mut self, text: String) -> u32 {
        let id = self.next_id;
        let stack = Stack::new(id, text);
        self.stacks.insert(id, stack);
        self.next_id += 1;
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
        self.stacks.insert(id, stack);
        self.next_id += 1;

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
            Some(stack) => Ok(stack),
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
}
