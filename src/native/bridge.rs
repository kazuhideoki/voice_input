//! Apple Music制御のためのRust-Swiftブリッジ

/// MediaPlayer権限の状態
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaLibraryAuthorizationStatus {
    NotDetermined = 0,
    Denied = 1,
    Restricted = 2,
    Authorized = 3,
}

impl From<i32> for MediaLibraryAuthorizationStatus {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::NotDetermined,
            1 => Self::Denied,
            2 => Self::Restricted,
            3 => Self::Authorized,
            _ => Self::NotDetermined,
        }
    }
}

/// 音楽再生状態
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MusicPlaybackState {
    Stopped = 0,
    Playing = 1,
    Paused = 2,
    Interrupted = 3,
    SeekingForward = 4,
    SeekingBackward = 5,
    Unknown = -1,
}

impl From<i32> for MusicPlaybackState {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Stopped,
            1 => Self::Playing,
            2 => Self::Paused,
            3 => Self::Interrupted,
            4 => Self::SeekingForward,
            5 => Self::SeekingBackward,
            _ => Self::Unknown,
        }
    }
}

// Swift関数のextern宣言（段階的移行中のため現在は無効化）
/*
unsafe extern "C" {
    fn pause_apple_music_native() -> bool;
    fn resume_apple_music_native() -> bool;
    fn get_music_playback_state() -> i32;
    fn get_media_library_authorization_status() -> i32;
    fn request_media_library_authorization() -> bool;
}
*/

/// ネイティブApple Music制御構造体
pub struct NativeMusicController;

impl NativeMusicController {
    /// Apple Musicを一時停止します（段階的移行中はダミー実装）
    /// 
    /// # Returns
    /// - `Err(_)`: 現在はswift-bridge実装が無効のため常にエラー
    pub fn pause_apple_music() -> Result<bool, Box<dyn std::error::Error>> {
        Err("Native Apple Music control is temporarily disabled during migration".into())
    }
    
    /// Apple Musicを再開します（段階的移行中はダミー実装）
    /// 
    /// # Returns
    /// - `Err(_)`: 現在はswift-bridge実装が無効のため常にエラー
    pub fn resume_apple_music() -> Result<bool, Box<dyn std::error::Error>> {
        Err("Native Apple Music control is temporarily disabled during migration".into())
    }
    
    /// 現在の音楽再生状態を取得します（段階的移行中はダミー実装）
    pub fn get_playback_state() -> Result<MusicPlaybackState, Box<dyn std::error::Error>> {
        Err("Native Apple Music control is temporarily disabled during migration".into())
    }
    
    /// 音楽が再生中かどうかを確認します（段階的移行中はダミー実装）
    pub fn is_playing() -> Result<bool, Box<dyn std::error::Error>> {
        Err("Native Apple Music control is temporarily disabled during migration".into())
    }
    
    /// MediaLibrary権限の状態を取得します（段階的移行中はダミー実装）
    pub fn get_authorization_status() -> Result<MediaLibraryAuthorizationStatus, Box<dyn std::error::Error>> {
        Err("Native Apple Music control is temporarily disabled during migration".into())
    }
    
    /// MediaLibrary権限を要求します（段階的移行中はダミー実装）
    pub fn request_authorization() -> Result<(), Box<dyn std::error::Error>> {
        Err("Native Apple Music control is temporarily disabled during migration".into())
    }
    
    /// 権限が利用可能かどうかを確認します（段階的移行中はダミー実装）
    pub fn is_authorized() -> Result<bool, Box<dyn std::error::Error>> {
        Err("Native Apple Music control is temporarily disabled during migration".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_library_authorization_status_conversion() {
        assert_eq!(MediaLibraryAuthorizationStatus::from(0), MediaLibraryAuthorizationStatus::NotDetermined);
        assert_eq!(MediaLibraryAuthorizationStatus::from(1), MediaLibraryAuthorizationStatus::Denied);
        assert_eq!(MediaLibraryAuthorizationStatus::from(2), MediaLibraryAuthorizationStatus::Restricted);
        assert_eq!(MediaLibraryAuthorizationStatus::from(3), MediaLibraryAuthorizationStatus::Authorized);
        assert_eq!(MediaLibraryAuthorizationStatus::from(999), MediaLibraryAuthorizationStatus::NotDetermined);
    }

    #[test]
    fn test_music_playback_state_conversion() {
        assert_eq!(MusicPlaybackState::from(0), MusicPlaybackState::Stopped);
        assert_eq!(MusicPlaybackState::from(1), MusicPlaybackState::Playing);
        assert_eq!(MusicPlaybackState::from(2), MusicPlaybackState::Paused);
        assert_eq!(MusicPlaybackState::from(-1), MusicPlaybackState::Unknown);
        assert_eq!(MusicPlaybackState::from(999), MusicPlaybackState::Unknown);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_native_music_controller_migration_state() {
        // 段階的移行中はエラーを返すことを確認
        let result = NativeMusicController::get_authorization_status();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("temporarily disabled"));
    }
}