// src/websocket_server.rs
use futures::{SinkExt, StreamExt};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message};

pub struct WebsocketServer {
    addr: String,
}

impl WebsocketServer {
    pub fn new(addr: &str) -> Self {
        WebsocketServer {
            addr: addr.to_string(),
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(&self.addr).await?;
        println!("WebSocketサーバーを開始: {}", self.addr);

        while let Ok((stream, addr)) = listener.accept().await {
            println!("クライアント接続: {}", addr);
            tokio::spawn(Self::handle_connection(stream));
        }

        Ok(())
    }

    async fn handle_connection(stream: TcpStream) -> Result<(), Box<dyn Error>> {
        let ws_stream = accept_async(stream).await?;
        println!("WebSocket接続確立");

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // クライアントからのメッセージを処理するタスク
        let sender_task = tokio::spawn(async move {
            let mut count = 0;

            // 段階的にメッセージを送信
            loop {
                count += 1;
                let message = format!("サーバーからの段階的メッセージ #{}", count);

                if let Err(e) = ws_sender.send(Message::Text(message)).await {
                    println!("メッセージ送信エラー: {}", e);
                    break;
                }

                println!("メッセージ #{} を送信しました", count);
                tokio::time::sleep(Duration::from_secs(2)).await;

                // 10回送信したら終了
                if count >= 10 {
                    break;
                }
            }
        });

        // クライアントからのメッセージを受信するタスク
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(msg) => {
                    if msg.is_text() || msg.is_binary() {
                        println!("クライアントから受信: {:?}", msg);
                    }
                }
                Err(e) => {
                    println!("受信エラー: {}", e);
                    break;
                }
            }
        }

        sender_task.await?;
        println!("WebSocket接続終了");
        Ok(())
    }
}
