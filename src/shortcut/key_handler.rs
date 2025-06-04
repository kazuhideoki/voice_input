//! キーイベント処理とIPC送信を担当するKeyHandler
//! rdev unstable_grab機能を使用してキーイベントを抑制し、IPCコマンドに変換
//! Phase 2: グローバル状態を排除してインスタンスベースのアーキテクチャに変更

use crate::ipc::IpcCmd;
use crate::shortcut::cmd_release_detector::CmdReleaseDetector;
use rdev::{Event, EventType, Key, grab};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// コールバック関数で使用する共有状態
#[derive(Clone)]
struct KeyHandlerState {
    cmd_pressed: Arc<Mutex<bool>>,
    ipc_sender: mpsc::UnboundedSender<IpcCmd>,
    cmd_detector: CmdReleaseDetector,
}

/// キーイベントを処理してIPCコマンドに変換するハンドラー
pub struct KeyHandler {
    ipc_sender: mpsc::UnboundedSender<IpcCmd>,
    cmd_pressed: Arc<Mutex<bool>>,
    cmd_detector: CmdReleaseDetector,
}

impl KeyHandler {
    /// 新しいKeyHandlerインスタンスを作成
    ///
    /// # Arguments
    /// * `ipc_sender` - IPCコマンドを送信するためのSender
    pub fn new(ipc_sender: mpsc::UnboundedSender<IpcCmd>) -> Self {
        Self::with_detector(ipc_sender, CmdReleaseDetector::new())
    }

    /// Cmdキー検出器を指定してKeyHandlerインスタンスを作成
    ///
    /// # Arguments
    /// * `ipc_sender` - IPCコマンドを送信するためのSender
    /// * `cmd_detector` - Cmdキーリリース検出器
    pub fn with_detector(
        ipc_sender: mpsc::UnboundedSender<IpcCmd>,
        cmd_detector: CmdReleaseDetector,
    ) -> Self {
        Self {
            ipc_sender,
            cmd_pressed: Arc::new(Mutex::new(false)),
            cmd_detector,
        }
    }

    /// キーイベントの抑制を開始
    ///
    /// # Returns
    /// * `Ok(())` - 正常に開始された場合
    /// * `Err(String)` - rdev::grabの開始に失敗した場合
    pub fn start_grab(self) -> Result<(), String> {
        println!("Starting keyboard event grabbing...");

        // インスタンスベースの共有状態を作成
        let shared_state = KeyHandlerState {
            cmd_pressed: self.cmd_pressed,
            ipc_sender: self.ipc_sender,
            cmd_detector: self.cmd_detector,
        };

        // rdev::grab開始 - クロージャーで共有状態をキャプチャ
        let event_handler = Self::create_event_handler(shared_state);

        if let Err(error) = grab(event_handler) {
            return Err(format!("キーイベント抑制の開始に失敗: {:?}", error));
        }

        Ok(())
    }

    /// イベントハンドラー関数を作成（クロージャーベース）
    ///
    /// # Arguments
    /// * `shared_state` - コールバック間で共有する状態
    ///
    /// # Returns
    /// * イベントハンドラー関数
    fn create_event_handler(shared_state: KeyHandlerState) -> impl Fn(Event) -> Option<Event> {
        move |event: Event| -> Option<Event> {
            let cmd_state = &shared_state.cmd_pressed;
            let ipc_sender = &shared_state.ipc_sender;
            let cmd_detector = &shared_state.cmd_detector;

            match event.event_type {
                EventType::KeyPress(key) => {
                    // Cmdキー状態更新
                    if Self::is_cmd_key(&key) {
                        if let Ok(mut pressed) = cmd_state.lock() {
                            *pressed = true;
                        }
                        cmd_detector.on_cmd_press();
                    }

                    // ESCキー処理（Cmdキー不要）
                    if key == Key::Escape {
                        let cmd = IpcCmd::DisableStackMode;
                        if let Err(e) = ipc_sender.send(cmd) {
                            eprintln!("Failed to send DisableStackMode command: {}", e);
                        } else {
                            println!("Sent DisableStackMode command (ESC)");
                        }
                        return None; // イベント抑制
                    }

                    // ショートカットキー判定とIPC送信
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
                                // Cmdキー状態を含めてPasteStackコマンドを送信
                                let number = Self::key_to_number(&key);
                                let cmd = IpcCmd::PasteStack { number };
                                if let Err(e) = ipc_sender.send(cmd) {
                                    eprintln!("Failed to send PasteStack command: {}", e);
                                } else {
                                    println!("Sent PasteStack command (Cmd+{})", number);
                                }
                                return None; // イベント抑制
                            }
                            Key::KeyC => {
                                // Cmd+Cで全スタッククリア
                                let cmd = IpcCmd::ClearStacks;
                                if let Err(e) = ipc_sender.send(cmd) {
                                    eprintln!("Failed to send ClearStacks command: {}", e);
                                } else {
                                    println!("Sent ClearStacks command (Cmd+C)");
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
                        cmd_detector.on_cmd_release();
                    }
                }
                _ => {}
            }

            Some(event) // パススルー
        }
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
        let handler = KeyHandler::new(_tx);

        // KeyHandlerが正常に作成されることを確認
        assert!(!KeyHandler::is_cmd_pressed(&handler.cmd_pressed));
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

    #[test]
    fn test_key_handler_state_structure() {
        let (_tx, _rx) = mpsc::unbounded_channel();
        let handler = KeyHandler::new(_tx.clone());

        // 共有状態が正しく作成されることを確認
        let shared_state = KeyHandlerState {
            cmd_pressed: handler.cmd_pressed.clone(),
            ipc_sender: _tx,
        };

        // 複製可能であることを確認
        let _cloned_state = shared_state.clone();

        // 初期状態が正しく設定されることを確認
        assert!(!KeyHandler::is_cmd_pressed(&shared_state.cmd_pressed));
    }

    #[test]
    fn test_multiple_key_handler_instances() {
        // 複数のKeyHandlerインスタンスが独立して動作することを確認
        let (_tx1, _rx1) = mpsc::unbounded_channel();
        let (_tx2, _rx2) = mpsc::unbounded_channel();

        let handler1 = KeyHandler::new(_tx1);
        let handler2 = KeyHandler::new(_tx2);

        // 各インスタンスが独立したcmd_pressedを持つことを確認
        {
            let mut pressed1 = handler1.cmd_pressed.lock().unwrap();
            *pressed1 = true;
        }

        assert!(KeyHandler::is_cmd_pressed(&handler1.cmd_pressed));
        assert!(!KeyHandler::is_cmd_pressed(&handler2.cmd_pressed));
    }

    #[test]
    fn test_escape_key_event_handling() {
        use rdev::{Event, EventType};

        // テスト用のチャンネル作成
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handler = KeyHandler::new(tx.clone());

        // 共有状態を作成
        let shared_state = KeyHandlerState {
            cmd_pressed: handler.cmd_pressed.clone(),
            ipc_sender: tx,
        };

        // イベントハンドラーを作成
        let event_handler = KeyHandler::create_event_handler(shared_state);

        // ESCキーイベントを作成
        let esc_event = Event {
            event_type: EventType::KeyPress(Key::Escape),
            time: std::time::SystemTime::now(),
            name: None,
        };

        // ESCキーイベントを処理
        let result = event_handler(esc_event);

        // ESCキーは抑制されるべき（None）
        assert!(result.is_none(), "ESCキーイベントは抑制されるべき");

        // DisableStackModeコマンドが送信されたことを確認
        let cmd = rx.blocking_recv();
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap(), IpcCmd::DisableStackMode);
    }

    #[test]
    fn test_escape_key_without_cmd() {
        use rdev::{Event, EventType};

        let (tx, mut rx) = mpsc::unbounded_channel();
        let handler = KeyHandler::new(tx.clone());

        let shared_state = KeyHandlerState {
            cmd_pressed: handler.cmd_pressed.clone(),
            ipc_sender: tx,
        };

        let event_handler = KeyHandler::create_event_handler(shared_state);

        // Cmdが押されていない状態でもESCキーは機能することを確認
        let esc_event = Event {
            event_type: EventType::KeyPress(Key::Escape),
            time: std::time::SystemTime::now(),
            name: None,
        };

        let result = event_handler(esc_event);
        assert!(result.is_none());

        let cmd = rx.blocking_recv();
        assert_eq!(cmd.unwrap(), IpcCmd::DisableStackMode);
    }
}
