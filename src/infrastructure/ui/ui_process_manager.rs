//! UIプロセスマネージャー（別プロセスGUI方式）
//!
//! macOSのEventLoop制約を回避するため、UIを別プロセスとして起動し、
//! Unix Socketを通じてデーモンと通信します。
//! voice_input_uiバイナリを子プロセスとして管理し、
//! スタック情報をリアルタイムで通知します。

use serde_json;
use std::process::{Child, Command, Stdio};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use super::types::{UiError, UiNotification};
use crate::application::stack_service::UiNotificationHandler;

pub struct UiProcessManager {
    ui_process: Option<Child>,
    socket_listener: Option<UnixListener>,
    notification_tx: Option<mpsc::UnboundedSender<UiNotification>>,
    is_running: bool,
}

impl UiProcessManager {
    pub fn new() -> Self {
        Self {
            ui_process: None,
            socket_listener: None,
            notification_tx: None,
            is_running: false,
        }
    }

    pub async fn start_ui(&mut self) -> Result<(), UiError> {
        if self.is_running {
            return Ok(());
        }

        // Unix Socketリスナーを作成
        let socket_path = "/tmp/voice_input_ui.sock";
        let _ = std::fs::remove_file(socket_path); // 既存ソケットを削除

        let listener = UnixListener::bind(socket_path).map_err(|e| {
            UiError::InitializationFailed(format!("Failed to bind UI socket: {}", e))
        })?;

        // UIプロセスを起動
        let ui_process = Command::new("cargo")
            .args(["run", "--bin", "voice_input_ui"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                UiError::InitializationFailed(format!("Failed to spawn UI process: {}", e))
            })?;

        // 通知チャネルを作成
        let (tx, rx) = mpsc::unbounded_channel();

        // UIプロセスからの接続を待機し、接続完了まで待つ
        let connection_task = tokio::spawn(async move {
            match listener.accept().await {
                Ok((stream, _)) => {
                    println!("UI process connected");
                    if let Err(e) = Self::handle_ui_connection(stream, rx).await {
                        eprintln!("UI connection handling failed: {:?}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to accept UI connection: {}", e);
                }
            }
        });

        // UIプロセスの接続を少し待つ（完全に待機する必要はない）
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        self.ui_process = Some(ui_process);
        // ソケットリスナーはタスクに移動されたので、Noneに設定
        self.socket_listener = None;
        self.notification_tx = Some(tx);
        self.is_running = true;

        // 接続タスクを保存（今は使わないが将来のクリーンアップ用）
        std::mem::drop(connection_task);

        println!("UI process started");
        Ok(())
    }

    async fn handle_ui_connection(
        stream: UnixStream,
        mut rx: mpsc::UnboundedReceiver<UiNotification>,
    ) -> Result<(), UiError> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // 通知をUIプロセスに送信するタスク
        let writer_task = tokio::spawn(async move {
            while let Some(notification) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&notification) {
                    if writer.write_all(json.as_bytes()).await.is_err() {
                        break;
                    }
                    if writer.write_all(b"\n").await.is_err() {
                        break;
                    }
                    if writer.flush().await.is_err() {
                        break;
                    }
                }
            }
        });

        // UIプロセスからのメッセージを受信（現在は使用しないが将来の拡張用）
        let reader_task = tokio::spawn(async move {
            let mut line = String::new();
            while reader.read_line(&mut line).await.is_ok() {
                if line.trim().is_empty() {
                    break;
                }
                line.clear();
            }
        });

        // 両方のタスクが完了するまで待機
        let _ = tokio::try_join!(writer_task, reader_task);

        Ok(())
    }

    pub fn notify(&self, notification: UiNotification) -> Result<(), UiError> {
        if let Some(tx) = &self.notification_tx {
            tx.send(notification).map_err(|_| UiError::ChannelClosed)?;
        }
        Ok(())
    }

    pub fn stop_ui(&mut self) -> Result<(), UiError> {
        if !self.is_running {
            return Ok(());
        }

        // 通知チャネルを閉じる
        self.notification_tx = None;

        // UIプロセスを終了
        if let Some(mut process) = self.ui_process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }

        // ソケットをクリーンアップ
        if let Some(_listener) = self.socket_listener.take() {
            let _ = std::fs::remove_file("/tmp/voice_input_ui.sock");
        }

        self.is_running = false;
        println!("UI process stopped");
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

impl Default for UiProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for UiProcessManager {
    fn drop(&mut self) {
        let _ = self.stop_ui();
    }
}

impl UiNotificationHandler for UiProcessManager {
    fn notify(&self, notification: UiNotification) -> Result<(), String> {
        self.notify(notification)
            .map_err(|e| format!("UI process notification failed: {:?}", e))
    }
}
