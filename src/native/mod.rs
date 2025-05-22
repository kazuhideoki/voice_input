//! ネイティブプラットフォーム機能モジュール
//! 
//! このモジュールはmacOS固有の機能を提供します。

#[cfg(target_os = "macos")]
pub mod bridge;

#[cfg(target_os = "macos")]
pub use bridge::{NativeMusicController, MediaLibraryAuthorizationStatus, MusicPlaybackState};

// macOS以外のプラットフォーム用のダミー実装
#[cfg(not(target_os = "macos"))]
pub struct NativeMusicController;

#[cfg(not(target_os = "macos"))]
impl NativeMusicController {
    pub fn pause_apple_music() -> Result<bool, Box<dyn std::error::Error>> {
        Err("Apple Music control is only available on macOS".into())
    }
    
    pub fn resume_apple_music() -> Result<bool, Box<dyn std::error::Error>> {
        Err("Apple Music control is only available on macOS".into())
    }
    
    pub fn is_playing() -> Result<bool, Box<dyn std::error::Error>> {
        Err("Apple Music control is only available on macOS".into())
    }
    
    pub fn is_authorized() -> Result<bool, Box<dyn std::error::Error>> {
        Err("Apple Music control is only available on macOS".into())
    }
    
    pub fn request_authorization() -> Result<(), Box<dyn std::error::Error>> {
        Err("Apple Music control is only available on macOS".into())
    }
}