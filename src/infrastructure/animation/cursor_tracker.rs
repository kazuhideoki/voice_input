//! Cursor position tracking implementation
//!
//! This module provides polling-based cursor position tracking
//! that will be used for positioning animation indicators.

use super::AnimationError;
use core_foundation_sys::base::{CFRelease, CFTypeRef};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// カーソル位置
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorPosition {
    pub x: f64,
    pub y: f64,
}

/// カーソル追跡器の実装（ポーリング方式のみ）
pub struct CursorTracker {
    sender: Sender<CursorPosition>,
    receiver: Receiver<CursorPosition>,
    tracking_thread: Option<thread::JoinHandle<()>>,
    should_stop: Arc<Mutex<bool>>,
}

impl Default for CursorTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// TODO カーソル位置以外も対応するなら trait を作ってもいいのかも
impl CursorTracker {
    /// 新しいカーソル追跡器を作成（30ms間隔のポーリング）
    pub fn new() -> Self {
        let (sender, receiver) = channel();
        Self {
            sender,
            receiver,
            tracking_thread: None,
            should_stop: Arc::new(Mutex::new(false)),
        }
    }

    /// 追跡を開始
    pub fn start(&mut self) -> Result<(), AnimationError> {
        if self.tracking_thread.is_some() {
            return Err(AnimationError::ThreadError("Already tracking".to_string()));
        }

        let sender = self.sender.clone();
        let should_stop = self.should_stop.clone();
        *should_stop.lock().unwrap() = false;

        let handle = thread::spawn(move || {
            while !*should_stop.lock().unwrap() {
                if let Ok(position) = Self::get_cursor_position_sync() {
                    let _ = sender.send(position);
                }
                thread::sleep(Duration::from_millis(30));
            }
        });

        self.tracking_thread = Some(handle);
        Ok(())
    }

    /// 追跡を停止
    pub fn stop(&mut self) -> Result<(), AnimationError> {
        *self.should_stop.lock().unwrap() = true;

        if let Some(handle) = self.tracking_thread.take() {
            handle
                .join()
                .map_err(|_| AnimationError::ThreadError("Failed to join thread".to_string()))?;
        }

        Ok(())
    }

    /// 現在のカーソル位置を同期的に取得
    fn get_cursor_position_sync() -> Result<CursorPosition, AnimationError> {
        // ポーリング実装では、常にマウスカーソル位置を直接取得する
        // AXUIElementの実装は複雑でテキストカーソル位置の取得が不安定なため、
        // Phase 1ではマウスカーソル位置のみを使用
        Self::get_mouse_position()
    }

    /// マウスカーソル位置を取得（フォールバック用）
    fn get_mouse_position() -> Result<CursorPosition, AnimationError> {
        #[repr(C)]
        struct CGPoint {
            x: f64,
            y: f64,
        }

        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
            fn CGEventGetLocation(event: *const std::ffi::c_void) -> CGPoint;
        }

        unsafe {
            let event = CGEventCreate(std::ptr::null());
            if event.is_null() {
                return Err(AnimationError::ResourceError(
                    "Failed to create CGEvent".to_string(),
                ));
            }

            let location = CGEventGetLocation(event);

            // CFRelease for event
            CFRelease(event as CFTypeRef);

            Ok(CursorPosition {
                x: location.x,
                y: location.y,
            })
        }
    }

    /// 位置更新を受信するためのレシーバーを取得
    pub fn get_receiver(&self) -> &Receiver<CursorPosition> {
        &self.receiver
    }
}

impl Drop for CursorTracker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
