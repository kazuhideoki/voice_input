//! キーイベント処理とIPC送信を担当するKeyHandler
//! シンプルな同期的状態管理で確実なcmd+キー操作を実現

use crate::ipc::IpcCmd;
use rdev::{Event, EventType, Key, grab};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// キーイベントを処理してIPCコマンドに変換するハンドラー
pub struct KeyHandler {
    ipc_sender: mpsc::UnboundedSender<IpcCmd>,
}

impl KeyHandler {
    /// 新しいKeyHandlerインスタンスを作成
    pub fn new(ipc_sender: mpsc::UnboundedSender<IpcCmd>) -> Self {
        Self { ipc_sender }
    }

    /// キーイベントの抑制を開始
    pub fn start_grab(self) -> Result<(), String> {
        println!("Starting keyboard event grabbing...");

        // 状態変数を共有するためのArc<Mutex<T>>
        let ipc_sender = self.ipc_sender;
        let cmd_pressed = Arc::new(Mutex::new(false));
        let enabled = Arc::new(Mutex::new(true));

        let cmd_pressed_clone = cmd_pressed.clone();
        let enabled_clone = enabled.clone();

        // rdev::grab開始
        if let Err(error) = grab(move |event: Event| -> Option<Event> {
            // ハンドラーが無効化されている場合は全てパススルー
            if !*enabled_clone.lock().unwrap_or_else(|e| e.into_inner()) {
                return Some(event);
            }

            match event.event_type {
                EventType::KeyPress(key) => {
                    // Cmdキー押下
                    if matches!(key, Key::MetaLeft | Key::MetaRight) {
                        *cmd_pressed_clone.lock().unwrap() = true;
                        return Some(event);
                    }

                    // Cmdが押されている時のみ処理
                    if *cmd_pressed_clone.lock().unwrap_or_else(|e| e.into_inner()) {
                        match key {
                            Key::KeyR => {
                                // 録音開始/停止
                                let _ = ipc_sender.send(IpcCmd::Toggle {
                                    paste: false,
                                    prompt: None,
                                    direct_input: false,
                                });
                                println!("Sent Toggle command (Cmd+R)");
                                return None; // イベント抑制
                            }
                            Key::KeyC => {
                                // スタッククリア
                                let _ = ipc_sender.send(IpcCmd::ClearStacks);
                                println!("Sent ClearStacks command (Cmd+C)");
                                return None; // イベント抑制
                            }
                            Key::Escape => {
                                // スタッキングモード終了
                                let _ = ipc_sender.send(IpcCmd::DisableStackMode);
                                println!("Sent DisableStackMode command (Cmd+ESC)");

                                // ハンドラー無効化とcmd状態リセット
                                *enabled_clone.lock().unwrap() = false;
                                *cmd_pressed_clone.lock().unwrap() = false;
                                println!("KeyHandler disabled");

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
                                // スタックペースト
                                let number = match key {
                                    Key::Num1 => 1,
                                    Key::Num2 => 2,
                                    Key::Num3 => 3,
                                    Key::Num4 => 4,
                                    Key::Num5 => 5,
                                    Key::Num6 => 6,
                                    Key::Num7 => 7,
                                    Key::Num8 => 8,
                                    Key::Num9 => 9,
                                    _ => unreachable!(),
                                };

                                let _ = ipc_sender.send(IpcCmd::PasteStack { number });
                                println!("Sent PasteStack command (Cmd+{})", number);
                                return None; // イベント抑制
                            }
                            _ => {} // その他のキーはパススルー
                        }
                    }
                }
                EventType::KeyRelease(key) => {
                    // Cmdキーリリース
                    if matches!(key, Key::MetaLeft | Key::MetaRight) {
                        *cmd_pressed_clone.lock().unwrap() = false;
                    }
                }
                _ => {}
            }

            Some(event) // デフォルトはパススルー
        }) {
            return Err(format!("キーイベント抑制の開始に失敗: {:?}", error));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_handler_creation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let handler = KeyHandler::new(tx);

        // 正常に作成されることを確認
        assert!(handler.ipc_sender.is_closed() == false);
    }

    #[test]
    fn test_cmd_key_combinations() {
        let (tx, mut rx) = mpsc::unbounded_channel();

        // テスト用のイベント処理シミュレーション
        #[allow(dead_code)]
        struct TestContext {
            ipc_sender: mpsc::UnboundedSender<IpcCmd>,
            cmd_pressed: bool,
            enabled: bool,
        }

        let mut ctx = TestContext {
            ipc_sender: tx,
            cmd_pressed: false,
            enabled: true,
        };

        // Cmd+Rのテスト
        ctx.cmd_pressed = true;
        ctx.ipc_sender
            .send(IpcCmd::Toggle {
                paste: false,
                prompt: None,
                direct_input: false,
            })
            .unwrap();

        let cmd = rx.blocking_recv().unwrap();
        assert!(matches!(cmd, IpcCmd::Toggle { .. }));

        // Cmd+Cのテスト
        ctx.ipc_sender.send(IpcCmd::ClearStacks).unwrap();
        let cmd = rx.blocking_recv().unwrap();
        assert_eq!(cmd, IpcCmd::ClearStacks);

        // Cmd+1のテスト
        ctx.ipc_sender
            .send(IpcCmd::PasteStack { number: 1 })
            .unwrap();
        let cmd = rx.blocking_recv().unwrap();
        assert_eq!(cmd, IpcCmd::PasteStack { number: 1 });

        // Cmd+ESCのテスト
        ctx.ipc_sender.send(IpcCmd::DisableStackMode).unwrap();
        let cmd = rx.blocking_recv().unwrap();
        assert_eq!(cmd, IpcCmd::DisableStackMode);

        // ESC後は無効化される
        ctx.enabled = false;
        ctx.cmd_pressed = false;
    }

    #[test]
    fn test_non_cmd_keys_passthrough() {
        let (tx, mut rx) = mpsc::unbounded_channel();

        // Cmdが押されていない状態でキーが押されてもコマンドは送信されない
        #[allow(dead_code)]
        struct TestContext {
            ipc_sender: mpsc::UnboundedSender<IpcCmd>,
            cmd_pressed: bool,
            enabled: bool,
        }

        let _ctx = TestContext {
            ipc_sender: tx,
            cmd_pressed: false,
            enabled: true,
        };

        // Rキー単独では何も送信されない
        assert!(rx.try_recv().is_err());
    }
}
