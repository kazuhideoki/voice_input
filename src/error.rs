//! 統一エラーハンドリング
//!
//! このモジュールは voice_input アプリケーション全体で使用する統一エラー型を定義します。
//! 既存の散在したエラー型を統合し、一貫したエラーハンドリングを提供します。

use crate::infrastructure::external::text_input_subprocess::SubprocessInputError;
use thiserror::Error;

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

    #[error("Audio backend error: {0}")]
    AudioBackendError(String),

    // ========================================
    // 転写関連エラー
    // ========================================
    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),

    // ========================================
    // テキスト入力エラー (SubprocessInputError統合)
    // ========================================
    // ========================================
    // IPC関連エラー
    // ========================================
    #[error("IPC connection failed: {0}")]
    IpcConnectionFailed(String),

    #[error("IPC serialization error: {0}")]
    IpcSerializationError(String),

    // ========================================
    // 設定関連エラー
    // ========================================
    #[error("Configuration initialization error: {0}")]
    ConfigInitError(String),

    #[error("System error: {0}")]
    SystemError(String),
}

/// 統一Result型エイリアス
pub type Result<T> = std::result::Result<T, VoiceInputError>;

// ========================================
// 既存エラー型からの自動変換実装
// ========================================

/// SubprocessInputError からの変換
impl From<SubprocessInputError> for VoiceInputError {
    fn from(error: SubprocessInputError) -> Self {
        match error {
            SubprocessInputError::SpawnError(msg)
            | SubprocessInputError::ExecutionError(msg)
            | SubprocessInputError::HelperNotFound(msg) => VoiceInputError::SystemError(msg),
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
        matches!(self, VoiceInputError::IpcConnectionFailed(_))
    }

    /// エラーがユーザーアクションで解決可能かどうかを判定
    pub fn is_user_actionable(&self) -> bool {
        matches!(self, VoiceInputError::ConfigInitError(_))
    }

    /// エラーの重要度レベルを取得（ログレベル代替）
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            VoiceInputError::ConfigInitError(_) => ErrorSeverity::Error,

            VoiceInputError::IpcConnectionFailed(_) => ErrorSeverity::Warning,

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
