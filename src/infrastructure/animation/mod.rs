//! Animation module for voice input visual feedback
//!
//! This module provides cursor tracking and animation functionality
//! for displaying visual indicators during voice recording.

use std::fmt;

/// アニメーション関連のエラー型
#[derive(Debug, Clone)]
pub enum AnimationError {
    /// アクセシビリティAPIエラー
    AccessibilityError(String),
    /// スレッド関連エラー
    ThreadError(String),
    /// リソース取得エラー
    ResourceError(String),
}

impl fmt::Display for AnimationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnimationError::AccessibilityError(msg) => write!(f, "Accessibility error: {}", msg),
            AnimationError::ThreadError(msg) => write!(f, "Thread error: {}", msg),
            AnimationError::ResourceError(msg) => write!(f, "Resource error: {}", msg),
        }
    }
}

impl std::error::Error for AnimationError {}

pub mod cursor_tracker;

pub use cursor_tracker::{CursorPosition, CursorTracker};
