//! 常駐ワーカー向けのテキスト入力インターフェース定義
//!
//! enigo を同一プロセスの別スレッドで常駐させる前提の型を提供する。

use async_trait::async_trait;
use enigo::{Direction::Release, Enigo, Key, Keyboard, Settings};
use std::fmt;
use tokio::sync::{mpsc, oneshot};

/// 常駐ワーカー用のテキスト入力エラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputWorkerError {
    /// Enigo 初期化失敗
    EnigoInitFailed(String),
    /// ワーカー起動失敗
    WorkerSpawnFailed(String),
    /// 入力実行失敗
    InputFailed(String),
    /// ワーカーとのチャネルが切断された
    ChannelClosed(String),
}

impl fmt::Display for TextInputWorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextInputWorkerError::EnigoInitFailed(msg) => {
                write!(f, "Enigo initialization failed: {}", msg)
            }
            TextInputWorkerError::WorkerSpawnFailed(msg) => {
                write!(f, "Text input worker spawn failed: {}", msg)
            }
            TextInputWorkerError::InputFailed(msg) => {
                write!(f, "Text input failed: {}", msg)
            }
            TextInputWorkerError::ChannelClosed(msg) => {
                write!(f, "Text input channel closed: {}", msg)
            }
        }
    }
}

impl std::error::Error for TextInputWorkerError {}

/// ワーカーへ送る入力リクエスト
#[derive(Debug)]
pub struct TextInputRequest {
    /// 入力するテキスト
    pub text: String,
    /// 完了通知用のチャネル
    pub completion: oneshot::Sender<Result<(), TextInputWorkerError>>,
}

/// テキスト入力エンジンのインターフェース
#[async_trait]
pub trait TextInputEngine: Send + Sync {
    /// テキストを入力する
    async fn type_text(&self, text: &str) -> Result<(), TextInputWorkerError>;
}

/// ワーカーへの送信ハンドル
#[derive(Clone)]
pub struct TextInputWorkerHandle {
    sender: mpsc::UnboundedSender<TextInputRequest>,
}

impl TextInputWorkerHandle {
    /// 新しいハンドルを作成
    pub fn new(sender: mpsc::UnboundedSender<TextInputRequest>) -> Self {
        Self { sender }
    }

    /// テキスト入力をリクエストし、完了通知の受信側を返す
    pub fn send(
        &self,
        text: String,
    ) -> Result<oneshot::Receiver<Result<(), TextInputWorkerError>>, TextInputWorkerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(TextInputRequest {
                text,
                completion: tx,
            })
            .map_err(|e| TextInputWorkerError::ChannelClosed(format!("send failed: {}", e)))?;
        Ok(rx)
    }
}

#[async_trait]
impl TextInputEngine for TextInputWorkerHandle {
    async fn type_text(&self, text: &str) -> Result<(), TextInputWorkerError> {
        let receiver = self.send(text.to_string())?;
        receiver.await.map_err(|_| {
            TextInputWorkerError::ChannelClosed("completion channel dropped".to_string())
        })?
    }
}

/// テキスト入力ワーカーを起動し、送信ハンドルを返す
pub fn start_text_input_worker() -> Result<TextInputWorkerHandle, TextInputWorkerError> {
    let (tx, rx) = mpsc::unbounded_channel::<TextInputRequest>();
    let handle = TextInputWorkerHandle::new(tx);

    if let Err(e) = std::thread::Builder::new()
        .name("text-input-worker".to_string())
        .spawn(move || run_worker(rx))
    {
        return Err(TextInputWorkerError::WorkerSpawnFailed(e.to_string()));
    }

    Ok(handle)
}

fn run_worker(mut rx: mpsc::UnboundedReceiver<TextInputRequest>) {
    let settings = Settings {
        mac_delay: 20,
        ..Default::default()
    };

    let mut enigo = match Enigo::new(&settings) {
        Ok(enigo) => enigo,
        Err(e) => {
            let msg = e.to_string();
            while let Some(req) = rx.blocking_recv() {
                let _ = req
                    .completion
                    .send(Err(TextInputWorkerError::EnigoInitFailed(msg.clone())));
            }
            return;
        }
    };

    while let Some(req) = rx.blocking_recv() {
        let result = type_text_with_enigo(&mut enigo, &req.text);
        let _ = req.completion.send(result);
    }
}

fn type_text_with_enigo(enigo: &mut Enigo, text: &str) -> Result<(), TextInputWorkerError> {
    // 少し待機
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Metaキーのリリース（念のため）
    let _ = enigo.key(Key::Meta, Release);

    // さらに待機
    std::thread::sleep(std::time::Duration::from_millis(30));

    // テキスト入力
    if let Err(e) = enigo.text(text) {
        return Err(TextInputWorkerError::InputFailed(e.to_string()));
    }

    // 完了待機
    std::thread::sleep(std::time::Duration::from_millis(30));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テキスト入力リクエストを送ると完了通知の受信側を受け取れる
    #[test]
    fn completion_receiver_is_returned_when_sending_request() {
        let (tx, mut rx) = mpsc::unbounded_channel::<TextInputRequest>();
        let handle = TextInputWorkerHandle::new(tx);

        let receiver = handle.send("hello".to_string());

        assert!(receiver.is_ok());
        let request = rx.try_recv().expect("request should be sent");
        assert_eq!(request.text, "hello");
    }

    /// 送信先が切断されている場合はチャネル切断エラーになる
    #[test]
    fn channel_closed_is_reported_when_sender_is_closed() {
        let (tx, rx) = mpsc::unbounded_channel::<TextInputRequest>();
        drop(rx);
        let handle = TextInputWorkerHandle::new(tx);

        let result = handle.send("hello".to_string());

        assert!(matches!(
            result,
            Err(TextInputWorkerError::ChannelClosed(_))
        ));
    }
}
