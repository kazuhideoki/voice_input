//! UI IPCクライアント
//!
//! UIプロセスがデーモンと通信するためのUnix Socketクライアント。
//! UI通知を受信し、スタック情報の更新をリアルタイムで反映します。

use serde_json;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use super::types::{UiError, UiNotification};

pub struct UiIpcClient {
    stream: UnixStream,
}

impl UiIpcClient {
    pub fn new(stream: UnixStream) -> Self {
        Self { stream }
    }

    pub async fn run(self, tx: mpsc::UnboundedSender<UiNotification>) -> Result<(), UiError> {
        let (reader, _writer) = self.stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        println!("UI IPC client started");

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // EOF: デーモンがソケットを閉じた
                    println!("Daemon closed UI socket connection");
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // JSON形式の通知をデシリアライズ
                    match serde_json::from_str::<UiNotification>(trimmed) {
                        Ok(notification) => {
                            if tx.send(notification).is_err() {
                                // UIアプリケーションが受信チャネルを閉じた
                                println!("UI application closed notification channel");
                                break;
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to parse UI notification: {} (raw: {})", e, trimmed);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error reading from UI socket: {}", e);
                    break;
                }
            }
        }

        println!("UI IPC client terminated");
        Ok(())
    }
}
