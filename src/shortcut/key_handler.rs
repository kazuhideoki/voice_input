//! キーイベント処理とIPC送信を担当するKeyHandler
//! rdev unstable_grab機能を使用してキーイベントを抑制し、IPCコマンドに変換

use crate::ipc::IpcCmd;
use rdev::{Event, EventType, Key, grab};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::mpsc;

// グローバル状態管理（Phase 0パターンを踏襲）
static CMD_PRESSED: OnceLock<Arc<Mutex<bool>>> = OnceLock::new();
static IPC_SENDER: OnceLock<mpsc::UnboundedSender<IpcCmd>> = OnceLock::new();

/// キーイベントを処理してIPCコマンドに変換するハンドラー
pub struct KeyHandler {
    ipc_sender: mpsc::UnboundedSender<IpcCmd>,
}

impl KeyHandler {
    /// 新しいKeyHandlerインスタンスを作成
    ///
    /// # Arguments
    /// * `ipc_sender` - IPCコマンドを送信するためのSender
    pub fn new(ipc_sender: mpsc::UnboundedSender<IpcCmd>) -> Self {
        Self { ipc_sender }
    }

    /// キーイベントの抑制を開始
    ///
    /// # Returns
    /// * `Ok(())` - 正常に開始された場合
    /// * `Err(String)` - rdev::grabの開始に失敗した場合
    pub fn start_grab(self) -> Result<(), String> {
        // グローバル状態の初期化（Phase 0のパターンを踏襲）
        CMD_PRESSED
            .set(Arc::new(Mutex::new(false)))
            .map_err(|_| "CMD_PRESSED already initialized".to_string())?;

        IPC_SENDER
            .set(self.ipc_sender)
            .map_err(|_| "IPC_SENDER already initialized".to_string())?;

        println!("Starting keyboard event grabbing...");

        // rdev::grab開始（Phase 0のkey_suppression_test.rsと同じ）
        if let Err(error) = grab(Self::handle_key_event) {
            return Err(format!("キーイベント抑制の開始に失敗: {:?}", error));
        }

        Ok(())
    }

    /// キーイベントのハンドリング関数（rdev::grabのコールバック）
    ///
    /// # Arguments
    /// * `event` - rdevから受信したキーイベント
    ///
    /// # Returns
    /// * `Some(event)` - イベントをパススルーする場合
    /// * `None` - イベントを抑制する場合
    fn handle_key_event(event: Event) -> Option<Event> {
        let cmd_state = CMD_PRESSED.get().unwrap();
        let ipc_sender = IPC_SENDER.get().unwrap();

        match event.event_type {
            EventType::KeyPress(key) => {
                // Cmdキー状態更新（Phase 0と同じ）
                if Self::is_cmd_key(&key) {
                    if let Ok(mut pressed) = cmd_state.lock() {
                        *pressed = true;
                    }
                }

                // ショートカットキー判定とIPC送信（既存コマンドを使用）
                if Self::is_cmd_pressed(cmd_state) {
                    match key {
                        Key::KeyR => {
                            // 既存のToggleコマンドを送信
                            let cmd = IpcCmd::Toggle {
                                paste: false,
                                prompt: None,
                                direct_input: false,
                            };
                            if let Err(e) = ipc_sender.send(cmd) {
                                eprintln!("Failed to send Toggle command: {}", e);
                            } else {
                                println!("Sent Toggle command (Cmd+R)");
                            }
                            return None; // イベント抑制
                        }
                        Key::Num1
                        | Key::Num2
                        | Key::Num3
                        | Key::Num4
                        | Key::Num5
                        | Key::Num6
                        | Key::Num7
                        | Key::Num8
                        | Key::Num9 => {
                            // 既存のPasteStackコマンドを送信
                            let number = Self::key_to_number(&key);
                            let cmd = IpcCmd::PasteStack { number };
                            if let Err(e) = ipc_sender.send(cmd) {
                                eprintln!("Failed to send PasteStack command: {}", e);
                            } else {
                                println!("Sent PasteStack command (Cmd+{})", number);
                            }
                            return None; // イベント抑制
                        }
                        _ => {}
                    }
                }
            }
            EventType::KeyRelease(key) => {
                if Self::is_cmd_key(&key) {
                    if let Ok(mut pressed) = cmd_state.lock() {
                        *pressed = false;
                    }
                }
            }
            _ => {}
        }

        Some(event) // パススルー
    }

    /// Cmdキー（Meta）の判定
    ///
    /// # Arguments
    /// * `key` - 判定するキー
    ///
    /// # Returns
    /// * `true` - Cmdキーの場合
    /// * `false` - Cmdキーでない場合
    fn is_cmd_key(key: &Key) -> bool {
        matches!(key, Key::MetaLeft | Key::MetaRight)
    }

    /// Cmdキーが押されているかチェック
    ///
    /// # Arguments
    /// * `cmd_state` - Cmdキーの状態を保持するMutex
    ///
    /// # Returns
    /// * `true` - Cmdキーが押されている場合
    /// * `false` - Cmdキーが押されていない場合
    fn is_cmd_pressed(cmd_state: &Arc<Mutex<bool>>) -> bool {
        cmd_state.lock().map(|pressed| *pressed).unwrap_or(false)
    }

    /// キーを数字に変換
    ///
    /// # Arguments
    /// * `key` - 変換するキー
    ///
    /// # Returns
    /// * `1-9` - 数字キーの場合
    /// * `0` - 数字キーでない場合
    fn key_to_number(key: &Key) -> u32 {
        match key {
            Key::Num1 => 1,
            Key::Num2 => 2,
            Key::Num3 => 3,
            Key::Num4 => 4,
            Key::Num5 => 5,
            Key::Num6 => 6,
            Key::Num7 => 7,
            Key::Num8 => 8,
            Key::Num9 => 9,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_is_cmd_key() {
        // Cmdキーの判定
        assert!(KeyHandler::is_cmd_key(&Key::MetaLeft));
        assert!(KeyHandler::is_cmd_key(&Key::MetaRight));

        // 非Cmdキーの判定
        assert!(!KeyHandler::is_cmd_key(&Key::KeyR));
        assert!(!KeyHandler::is_cmd_key(&Key::Num1));
        assert!(!KeyHandler::is_cmd_key(&Key::ControlLeft));
        assert!(!KeyHandler::is_cmd_key(&Key::ShiftLeft));
    }

    #[test]
    fn test_key_to_number() {
        // 数字キーの変換
        assert_eq!(KeyHandler::key_to_number(&Key::Num1), 1);
        assert_eq!(KeyHandler::key_to_number(&Key::Num2), 2);
        assert_eq!(KeyHandler::key_to_number(&Key::Num3), 3);
        assert_eq!(KeyHandler::key_to_number(&Key::Num4), 4);
        assert_eq!(KeyHandler::key_to_number(&Key::Num5), 5);
        assert_eq!(KeyHandler::key_to_number(&Key::Num6), 6);
        assert_eq!(KeyHandler::key_to_number(&Key::Num7), 7);
        assert_eq!(KeyHandler::key_to_number(&Key::Num8), 8);
        assert_eq!(KeyHandler::key_to_number(&Key::Num9), 9);

        // 非数字キーは0を返す
        assert_eq!(KeyHandler::key_to_number(&Key::KeyR), 0);
        assert_eq!(KeyHandler::key_to_number(&Key::MetaLeft), 0);
        assert_eq!(KeyHandler::key_to_number(&Key::Space), 0);
        assert_eq!(KeyHandler::key_to_number(&Key::Num0), 0);
    }

    #[test]
    fn test_key_handler_creation() {
        let (_tx, _rx) = mpsc::unbounded_channel();
        let _handler = KeyHandler::new(_tx);

        // KeyHandlerが正常に作成されることを確認
        // 実際のgrab機能はテストしない（アクセシビリティ権限が必要）
    }

    #[test]
    fn test_cmd_state_logic() {
        let cmd_state = Arc::new(Mutex::new(false));

        // 初期状態
        assert!(!KeyHandler::is_cmd_pressed(&cmd_state));

        // Cmd押下状態をシミュレート
        {
            let mut pressed = cmd_state.lock().unwrap();
            *pressed = true;
        }
        assert!(KeyHandler::is_cmd_pressed(&cmd_state));

        // Cmdリリース状態をシミュレート
        {
            let mut pressed = cmd_state.lock().unwrap();
            *pressed = false;
        }
        assert!(!KeyHandler::is_cmd_pressed(&cmd_state));
    }
}
