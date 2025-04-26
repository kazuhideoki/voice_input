//! Unix Domain Socket を使ったシンプル IPC
use serde::{Deserialize, Serialize};
use std::{error::Error, path::Path};

pub const SOCKET_PATH: &str = "/tmp/voice_input.sock";

/// -------- コマンド ----------
#[derive(Debug, Serialize, Deserialize)]
pub enum IpcCmd {
    Start { paste: bool, prompt: Option<String> },
    Stop,
    Toggle { paste: bool, prompt: Option<String> },
    Status,
}

/// -------- レスポンス --------
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResp {
    pub ok: bool,
    pub msg: String,
}

/// -------- クライアント送信ユーティリティ --------
pub fn send_cmd(cmd: &IpcCmd) -> Result<IpcResp, Box<dyn Error>> {
    use futures::{SinkExt, StreamExt};
    use tokio::net::UnixStream;
    use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec}; // 🆕 追加

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            if !Path::new(SOCKET_PATH).exists() {
                return Err("daemon socket not found".into());
            }

            let stream = UnixStream::connect(SOCKET_PATH).await?;
            let (r, w) = stream.into_split();
            let mut writer = FramedWrite::new(w, LinesCodec::new());
            let mut reader = FramedRead::new(r, LinesCodec::new());

            let json = serde_json::to_string(cmd)?;
            writer.send(json).await?; // SinkExt::send

            if let Some(Ok(line)) = reader.next().await {
                // StreamExt::next
                let resp: IpcResp = serde_json::from_str(&line)?;
                Ok(resp)
            } else {
                Err("no response from daemon".into())
            }
        })
}
