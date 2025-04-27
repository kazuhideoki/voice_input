//! Unix Domain Socket (UDS) ベースのシンプルな IPC モジュール。
//! `voice_input` CLI ↔ `voice_inputd` デーモン間の通信で利用します。
use serde::{Deserialize, Serialize};
use std::{error::Error, path::Path};

/// デーモンソケットパス
pub const SOCKET_PATH: &str = "/tmp/voice_input.sock";

/// CLI からデーモンへ送るコマンド列挙。
#[derive(Debug, Serialize, Deserialize)]
pub enum IpcCmd {
    /// 録音開始
    Start { paste: bool, prompt: Option<String> },
    /// 録音停止
    Stop,
    /// 録音トグル
    Toggle { paste: bool, prompt: Option<String> },
    /// ステータス取得
    Status,
}

/// デーモンからの汎用レスポンス。
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResp {
    pub ok: bool,
    pub msg: String,
}

/// コマンドを送信して `IpcResp` を取得する同期ユーティリティ。
pub fn send_cmd(cmd: &IpcCmd) -> Result<IpcResp, Box<dyn Error>> {
    use futures::{SinkExt, StreamExt};
    use tokio::net::UnixStream;
    use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

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

            writer.send(serde_json::to_string(cmd)?).await?;
            if let Some(Ok(line)) = reader.next().await {
                Ok(serde_json::from_str::<IpcResp>(&line)?)
            } else {
                Err("no response from daemon".into())
            }
        })
}
