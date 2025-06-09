//! 統一エラーハンドリング
//!
//! このモジュールは voice_input アプリケーション全体で使用する統一エラー型を定義します。
//! 既存の散在したエラー型を統合し、一貫したエラーハンドリングを提供します。

use thiserror::Error;
use crate::infrastructure::external::text_input_subprocess::SubprocessInputError;

/// voice_input アプリケーション全体で使用する統一エラー型
#[derive(Debug, Error)]
pub enum VoiceInputError {
    // ========================================
    // 録音関連エラー
    // ========================================
    #[error("Recording not started")]
    RecordingNotStarted,

    #[error("Recording already active")]
    RecordingAlreadyActive,

    #[error("Audio device error: {0}")]
    AudioDeviceError(String),

    #[error("Audio backend error: {0}")]
    AudioBackendError(String),

    // ========================================
    // 転写関連エラー
    // ========================================
    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),

    #[error("OpenAI API error: {0}")]
    OpenAiApiError(String),

    #[error("OpenAI configuration error: {0}")]
    OpenAiConfigError(String),

    // ========================================
    // スタック管理エラー (StackServiceError統合)
    // ========================================
    #[error("Stack not found: id={stack_id}, available={available_ids:?}")]
    StackNotFound { stack_id: u32, available_ids: Vec<u32> },

    #[error("Stack mode is disabled")]
    StackModeDisabled,

    #[error("Text too large: {size} characters")]
    TextTooLarge { size: usize },

    // ========================================
    // UI関連エラー (UiError統合)
    // ========================================
    #[error("UI initialization failed: {0}")]
    UiInitializationFailed(String),

    #[error("UI channel closed")]
    UiChannelClosed,

    #[error("UI rendering error: {0}")]
    UiRenderingError(String),

    // ========================================
    // ショートカット関連エラー (ShortcutError統合)
    // ========================================
    #[error("Shortcut initialization failed: {0}")]
    ShortcutInitFailed(String),

    #[error("Shortcut system requirement not met: {0}")]
    ShortcutSystemRequirementNotMet(String),

    #[error("Shortcut IPC channel closed")]
    ShortcutIpcChannelClosed,

    // ========================================
    // テキスト入力エラー (SubprocessInputError統合)
    // ========================================
    #[error("Text input spawn error: {0}")]
    TextInputSpawnError(String),

    #[error("Text input execution error: {0}")]
    TextInputExecutionError(String),

    #[error("Text input helper not found: {0}")]
    TextInputHelperNotFound(String),

    // ========================================
    // IPC関連エラー
    // ========================================
    #[error("IPC connection failed: {0}")]
    IpcConnectionFailed(String),

    #[error("IPC serialization error: {0}")]
    IpcSerializationError(String),

    #[error("IPC channel closed")]
    IpcChannelClosed,

    // ========================================
    // 設定関連エラー
    // ========================================
    #[error("Configuration initialization error: {0}")]
    ConfigInitError(String),

    #[error("Configuration missing value: {0}")]
    ConfigMissingValue(String),

    // ========================================
    // ファイルI/O関連エラー
    // ========================================
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("File read error: {path}: {source}")]
    FileReadError { path: String, source: std::io::Error },

    #[error("File write error: {path}: {source}")]
    FileWriteError { path: String, source: std::io::Error },

    // ========================================
    // システム関連エラー
    // ========================================
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("System error: {0}")]
    SystemError(String),

    #[error("Permission denied: {reason}")]
    PermissionDenied { reason: String },

    // ========================================
    // 外部ライブラリからの自動変換
    // ========================================
    #[error("Standard I/O error")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error")]
    SerdeJson(#[from] serde_json::Error),

    #[error("HTTP request error")]
    Reqwest(#[from] reqwest::Error),
}

/// 統一Result型エイリアス
pub type Result<T> = std::result::Result<T, VoiceInputError>;

// ========================================
// 既存エラー型からの自動変換実装
// ========================================

/// StackServiceError からの変換
impl From<crate::application::StackServiceError> for VoiceInputError {
    fn from(error: crate::application::StackServiceError) -> Self {
        match error {
            crate::application::StackServiceError::StackNotFound(stack_id, available_ids) => {
                VoiceInputError::StackNotFound { stack_id, available_ids }
            }
            crate::application::StackServiceError::StackModeDisabled => {
                VoiceInputError::StackModeDisabled
            }
            crate::application::StackServiceError::TextTooLarge(size) => {
                VoiceInputError::TextTooLarge { size }
            }
        }
    }
}

/// UiError からの変換
impl From<crate::infrastructure::ui::UiError> for VoiceInputError {
    fn from(error: crate::infrastructure::ui::UiError) -> Self {
        match error {
            crate::infrastructure::ui::UiError::InitializationFailed(msg) => {
                VoiceInputError::UiInitializationFailed(msg)
            }
            crate::infrastructure::ui::UiError::ChannelClosed => {
                VoiceInputError::UiChannelClosed
            }
            crate::infrastructure::ui::UiError::RenderingError(msg) => {
                VoiceInputError::UiRenderingError(msg)
            }
        }
    }
}

/// ShortcutError からの変換
impl From<crate::shortcut::ShortcutError> for VoiceInputError {
    fn from(error: crate::shortcut::ShortcutError) -> Self {
        match error {
            crate::shortcut::ShortcutError::RdevInitFailed(msg) => {
                VoiceInputError::ShortcutInitFailed(msg)
            }
            crate::shortcut::ShortcutError::IpcChannelClosed => {
                VoiceInputError::ShortcutIpcChannelClosed
            }
            crate::shortcut::ShortcutError::SystemRequirementNotMet(msg) => {
                VoiceInputError::ShortcutSystemRequirementNotMet(msg)
            }
        }
    }
}

/// SubprocessInputError からの変換
impl From<SubprocessInputError> for VoiceInputError {
    fn from(error: SubprocessInputError) -> Self {
        match error {
            SubprocessInputError::SpawnError(msg) => {
                VoiceInputError::TextInputSpawnError(msg)
            }
            SubprocessInputError::ExecutionError(msg) => {
                VoiceInputError::TextInputExecutionError(msg)
            }
            SubprocessInputError::HelperNotFound(msg) => {
                VoiceInputError::TextInputHelperNotFound(msg)
            }
        }
    }
}

// ========================================
// 後方互換性の維持  
// ========================================

/// String からの変換（既存の文字列エラーとの互換性）
impl From<String> for VoiceInputError {
    fn from(message: String) -> Self {
        VoiceInputError::SystemError(message)
    }
}

/// &str からの変換（便利メソッド）
impl From<&str> for VoiceInputError {
    fn from(message: &str) -> Self {
        VoiceInputError::SystemError(message.to_string())
    }
}

/// String への変換（既存の文字列エラーとの互換性）
impl From<VoiceInputError> for String {
    fn from(error: VoiceInputError) -> Self {
        error.to_string()
    }
}

// ========================================
// ヘルパー関数
// ========================================

impl VoiceInputError {
    /// エラーが再試行可能かどうかを判定
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            VoiceInputError::NetworkError(_)
                | VoiceInputError::OpenAiApiError(_)
                | VoiceInputError::IpcConnectionFailed(_)
                | VoiceInputError::Reqwest(_)
        )
    }

    /// エラーがユーザーアクションで解決可能かどうかを判定
    pub fn is_user_actionable(&self) -> bool {
        matches!(
            self,
            VoiceInputError::ConfigMissingValue(_)
                | VoiceInputError::OpenAiConfigError(_)
                | VoiceInputError::PermissionDenied { .. }
                | VoiceInputError::TextInputHelperNotFound(_)
        )
    }

    /// エラーの重要度レベルを取得（ログレベル代替）
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            VoiceInputError::PermissionDenied { .. }
            | VoiceInputError::ConfigMissingValue(_)
            | VoiceInputError::FileNotFound { .. } => ErrorSeverity::Error,

            VoiceInputError::NetworkError(_)
            | VoiceInputError::OpenAiApiError(_)
            | VoiceInputError::IpcConnectionFailed(_) => ErrorSeverity::Warning,

            VoiceInputError::UiChannelClosed
            | VoiceInputError::IpcChannelClosed
            | VoiceInputError::ShortcutIpcChannelClosed => ErrorSeverity::Info,

            _ => ErrorSeverity::Debug,
        }
    }
}

/// エラーの重要度レベル
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Debug,
    Info,
    Warning,
    Error,
}