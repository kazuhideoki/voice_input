// src/websocket_client.rs
use futures::{SinkExt, StreamExt};
use std::error::Error;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

pub struct WebsocketClient {
    url: String,
}

impl WebsocketClient {
    pub fn new(url: &str) -> Self {
        WebsocketClient {
            url: url.to_string(),
        }
    }

    pub async fn connect(&self) -> Result<(), Box<dyn Error>> {
        let url = Url::parse(&self.url)?;

        println!("サーバーに接続中: {}", url);
        let (ws_stream, _) = connect_async(url).await?;
        println!("WebSocketサーバーに接続しました");

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // 初期メッセージを送信
        ws_sender
            .send(Message::Text(
                "クライアントからの接続確認メッセージ".to_string(),
            ))
            .await?;

        // サーバーからのメッセージを受信
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(msg) => {
                    println!("サーバーから受信: {:?}", msg);

                    // サーバーに応答
                    ws_sender
                        .send(Message::Text("メッセージを受信しました".to_string()))
                        .await?;
                }
                Err(e) => {
                    println!("エラー: {}", e);
                    break;
                }
            }
        }

        println!("接続を終了します");
        Ok(())
    }
}
