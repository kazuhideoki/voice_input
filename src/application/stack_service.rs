use crate::domain::stack::{Stack, StackInfo};
use std::collections::HashMap;

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

    /// 指定番号のスタックを取得
    pub fn get_stack(&self, number: u32) -> Option<&Stack> {
        self.stacks.get(&number)
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
