//! UI関連の型定義
//!
//! スタック表示用の情報とUI通知システムに使用される型を定義します。
//! - StackDisplayInfo: UIに表示するスタック情報
//! - UiNotification: UI更新イベント
//! - UiState: UI状態管理
//! - UiError: UIエラー処理

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackDisplayInfo {
    pub number: u32,
    pub preview: String,
    pub created_at: String,
    pub is_active: bool,
    pub char_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiState {
    pub stack_mode_enabled: bool,
    pub stacks: Vec<StackDisplayInfo>,
    pub total_count: usize,
    pub last_accessed_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiNotification {
    StackAdded(StackDisplayInfo),
    StackAccessed(u32),
    StacksCleared,
    ModeChanged(bool),
}

#[derive(Debug, Clone)]
pub enum UiError {
    InitializationFailed(String),
    ChannelClosed,
    RenderingError(String),
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UiError::InitializationFailed(msg) => write!(f, "UI initialization failed: {}", msg),
            UiError::ChannelClosed => write!(f, "UI communication channel closed"),
            UiError::RenderingError(msg) => write!(f, "UI rendering error: {}", msg),
        }
    }
}

impl std::error::Error for UiError {}
