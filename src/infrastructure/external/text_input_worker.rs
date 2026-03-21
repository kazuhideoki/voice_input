//! 常駐ワーカー向けのテキスト入力インターフェース定義
//!
//! enigo を同一プロセスの別スレッドで常駐させる前提の型を提供する。

use async_trait::async_trait;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};
use tokio::sync::{mpsc, oneshot};

/// 常駐ワーカー用のテキスト入力エラー
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextInputWorkerError {
    /// Enigo 初期化失敗
    #[error("Enigo initialization failed: {0}")]
    EnigoInitFailed(String),
    /// ワーカー起動失敗
    #[error("Text input worker spawn failed: {0}")]
    WorkerSpawnFailed(String),
    /// 入力実行失敗
    #[error("Text input failed: {0}")]
    InputFailed(String),
    /// ワーカーとのチャネルが切断された
    #[error("Text input channel closed: {0}")]
    ChannelClosed(String),
}

/// ワーカーへ送る入力リクエスト
#[derive(Debug)]
pub enum TextInputRequest {
    /// テキストをそのまま入力
    TypeText {
        /// 入力するテキスト
        text: String,
        /// 入力実行モード
        mode: TextInputExecutionMode,
        /// 完了通知用のチャネル
        completion: oneshot::Sender<Result<(), TextInputWorkerError>>,
    },
    /// 入力済みテキストの末尾を削って差分を入力
    ReplaceSuffix {
        /// 削除する文字数
        delete_count: usize,
        /// 追加するテキスト
        text: String,
        /// 入力実行モード
        mode: TextInputExecutionMode,
        /// 完了通知用のチャネル
        completion: oneshot::Sender<Result<(), TextInputWorkerError>>,
    },
    /// 直近に入力したテキスト範囲を選択する
    SelectRecentRange {
        /// カーソル末尾から左へ戻る文字数
        trailing_char_count: usize,
        /// 選択する文字数
        char_count: usize,
        /// 完了通知用のチャネル
        completion: oneshot::Sender<Result<(), TextInputWorkerError>>,
    },
}

impl TextInputRequest {
    /// 完了通知用のチャネル
    fn completion(self) -> oneshot::Sender<Result<(), TextInputWorkerError>> {
        match self {
            TextInputRequest::TypeText { completion, .. }
            | TextInputRequest::ReplaceSuffix { completion, .. }
            | TextInputRequest::SelectRecentRange { completion, .. } => completion,
        }
    }
}

/// テキスト入力の実行モード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputExecutionMode {
    /// 単発入力として準備と待機を含めて処理する
    Standalone,
    /// 連続入力として追加の待機を省いて処理する
    Continuous,
}

/// テキスト入力エンジンのインターフェース
#[async_trait]
pub trait TextInputEngine: Send + Sync {
    /// テキストを入力する
    async fn type_text(&self, text: &str) -> Result<(), TextInputWorkerError>;

    /// 連続入力の一部としてテキストを入力する
    async fn type_text_continuous(&self, text: &str) -> Result<(), TextInputWorkerError>;

    /// 入力済みテキストの末尾差分を置き換える
    async fn replace_suffix(
        &self,
        delete_count: usize,
        text: &str,
    ) -> Result<(), TextInputWorkerError>;

    /// 連続入力の一部として末尾差分を置き換える
    async fn replace_suffix_continuous(
        &self,
        delete_count: usize,
        text: &str,
    ) -> Result<(), TextInputWorkerError>;

    /// 直近に入力したテキスト範囲を選択する
    async fn select_recent_range(
        &self,
        trailing_char_count: usize,
        char_count: usize,
    ) -> Result<(), TextInputWorkerError>;
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
        self.send_with_mode(text, TextInputExecutionMode::Standalone)
    }

    /// 連続入力をリクエストし、完了通知の受信側を返す
    pub fn send_continuous(
        &self,
        text: String,
    ) -> Result<oneshot::Receiver<Result<(), TextInputWorkerError>>, TextInputWorkerError> {
        self.send_with_mode(text, TextInputExecutionMode::Continuous)
    }

    fn send_with_mode(
        &self,
        text: String,
        mode: TextInputExecutionMode,
    ) -> Result<oneshot::Receiver<Result<(), TextInputWorkerError>>, TextInputWorkerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(TextInputRequest::TypeText {
                text,
                mode,
                completion: tx,
            })
            .map_err(|e| TextInputWorkerError::ChannelClosed(format!("send failed: {}", e)))?;
        Ok(rx)
    }

    /// 差分置き換えをリクエストし、完了通知の受信側を返す
    pub fn send_replace_suffix(
        &self,
        delete_count: usize,
        text: String,
    ) -> Result<oneshot::Receiver<Result<(), TextInputWorkerError>>, TextInputWorkerError> {
        self.send_replace_suffix_with_mode(delete_count, text, TextInputExecutionMode::Standalone)
    }

    /// 連続入力用の差分置き換えをリクエストし、完了通知の受信側を返す
    pub fn send_replace_suffix_continuous(
        &self,
        delete_count: usize,
        text: String,
    ) -> Result<oneshot::Receiver<Result<(), TextInputWorkerError>>, TextInputWorkerError> {
        self.send_replace_suffix_with_mode(delete_count, text, TextInputExecutionMode::Continuous)
    }

    fn send_replace_suffix_with_mode(
        &self,
        delete_count: usize,
        text: String,
        mode: TextInputExecutionMode,
    ) -> Result<oneshot::Receiver<Result<(), TextInputWorkerError>>, TextInputWorkerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(TextInputRequest::ReplaceSuffix {
                delete_count,
                text,
                mode,
                completion: tx,
            })
            .map_err(|e| TextInputWorkerError::ChannelClosed(format!("send failed: {}", e)))?;
        Ok(rx)
    }

    /// 直近に入力したテキスト範囲の選択をリクエストする
    pub fn send_select_recent_range(
        &self,
        trailing_char_count: usize,
        char_count: usize,
    ) -> Result<oneshot::Receiver<Result<(), TextInputWorkerError>>, TextInputWorkerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(TextInputRequest::SelectRecentRange {
                trailing_char_count,
                char_count,
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

    async fn type_text_continuous(&self, text: &str) -> Result<(), TextInputWorkerError> {
        let receiver = self.send_continuous(text.to_string())?;
        receiver.await.map_err(|_| {
            TextInputWorkerError::ChannelClosed("completion channel dropped".to_string())
        })?
    }

    async fn replace_suffix(
        &self,
        delete_count: usize,
        text: &str,
    ) -> Result<(), TextInputWorkerError> {
        let receiver = self.send_replace_suffix(delete_count, text.to_string())?;
        receiver.await.map_err(|_| {
            TextInputWorkerError::ChannelClosed("completion channel dropped".to_string())
        })?
    }

    async fn replace_suffix_continuous(
        &self,
        delete_count: usize,
        text: &str,
    ) -> Result<(), TextInputWorkerError> {
        let receiver = self.send_replace_suffix_continuous(delete_count, text.to_string())?;
        receiver.await.map_err(|_| {
            TextInputWorkerError::ChannelClosed("completion channel dropped".to_string())
        })?
    }

    async fn select_recent_range(
        &self,
        trailing_char_count: usize,
        char_count: usize,
    ) -> Result<(), TextInputWorkerError> {
        let receiver = self.send_select_recent_range(trailing_char_count, char_count)?;
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
    let settings = Settings::default();

    let mut enigo = match Enigo::new(&settings) {
        Ok(enigo) => enigo,
        Err(e) => {
            let msg = e.to_string();
            while let Some(req) = rx.blocking_recv() {
                let _ = req
                    .completion()
                    .send(Err(TextInputWorkerError::EnigoInitFailed(msg.clone())));
            }
            return;
        }
    };

    while let Some(req) = rx.blocking_recv() {
        match req {
            TextInputRequest::TypeText {
                text,
                mode,
                completion,
            } => {
                let result = type_text_with_enigo(&mut enigo, &text, mode);
                let _ = completion.send(result);
            }
            TextInputRequest::ReplaceSuffix {
                delete_count,
                text,
                mode,
                completion,
            } => {
                let result = replace_suffix_with_enigo(&mut enigo, delete_count, &text, mode);
                let _ = completion.send(result);
            }
            TextInputRequest::SelectRecentRange {
                trailing_char_count,
                char_count,
                completion,
            } => {
                let result =
                    select_recent_range_with_enigo(&mut enigo, trailing_char_count, char_count);
                let _ = completion.send(result);
            }
        }
    }
}

fn type_text_with_enigo(
    enigo: &mut Enigo,
    text: &str,
    mode: TextInputExecutionMode,
) -> Result<(), TextInputWorkerError> {
    if mode == TextInputExecutionMode::Standalone {
        prepare_input(enigo)?;
    }
    input_text(enigo, text, mode)
}

fn replace_suffix_with_enigo(
    enigo: &mut Enigo,
    delete_count: usize,
    text: &str,
    mode: TextInputExecutionMode,
) -> Result<(), TextInputWorkerError> {
    if mode == TextInputExecutionMode::Standalone {
        prepare_input(enigo)?;
    }

    for _ in 0..delete_count {
        enigo
            .key(Key::Backspace, Click)
            .map_err(|e| TextInputWorkerError::InputFailed(e.to_string()))?;
    }

    input_text(enigo, text, mode)
}

fn prepare_input(enigo: &mut Enigo) -> Result<(), TextInputWorkerError> {
    std::thread::sleep(std::time::Duration::from_millis(50));
    enigo
        .key(Key::Meta, Release)
        .map_err(|e| TextInputWorkerError::InputFailed(e.to_string()))?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    Ok(())
}

fn input_text(
    enigo: &mut Enigo,
    text: &str,
    mode: TextInputExecutionMode,
) -> Result<(), TextInputWorkerError> {
    if let Err(e) = enigo.text(text) {
        return Err(TextInputWorkerError::InputFailed(e.to_string()));
    }

    if mode == TextInputExecutionMode::Standalone {
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    Ok(())
}

fn select_recent_range_with_enigo(
    enigo: &mut Enigo,
    trailing_char_count: usize,
    char_count: usize,
) -> Result<(), TextInputWorkerError> {
    if char_count == 0 {
        return Ok(());
    }

    prepare_input(enigo)?;

    for _ in 0..trailing_char_count {
        enigo
            .key(Key::LeftArrow, Click)
            .map_err(|e| TextInputWorkerError::InputFailed(e.to_string()))?;
    }

    enigo
        .key(Key::Shift, Press)
        .map_err(|e| TextInputWorkerError::InputFailed(e.to_string()))?;
    let selection_result = (|| -> Result<(), TextInputWorkerError> {
        for _ in 0..char_count {
            enigo
                .key(Key::LeftArrow, Click)
                .map_err(|e| TextInputWorkerError::InputFailed(e.to_string()))?;
        }
        Ok(())
    })();

    let release_result = enigo
        .key(Key::Shift, Release)
        .map_err(|e| TextInputWorkerError::InputFailed(e.to_string()));
    selection_result?;
    release_result?;
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
        match request {
            TextInputRequest::TypeText { text, mode, .. } => {
                assert_eq!(text, "hello");
                assert_eq!(mode, TextInputExecutionMode::Standalone);
            }
            TextInputRequest::ReplaceSuffix { .. } | TextInputRequest::SelectRecentRange { .. } => {
                panic!("unexpected request")
            }
        }
    }

    /// 連続入力リクエストは継続モードで送信される
    #[test]
    fn continuous_type_request_uses_continuous_mode() {
        let (tx, mut rx) = mpsc::unbounded_channel::<TextInputRequest>();
        let handle = TextInputWorkerHandle::new(tx);

        let receiver = handle.send_continuous("hello".to_string());

        assert!(receiver.is_ok());
        let request = rx.try_recv().expect("request should be sent");
        match request {
            TextInputRequest::TypeText { text, mode, .. } => {
                assert_eq!(text, "hello");
                assert_eq!(mode, TextInputExecutionMode::Continuous);
            }
            TextInputRequest::ReplaceSuffix { .. } | TextInputRequest::SelectRecentRange { .. } => {
                panic!("unexpected request")
            }
        }
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

    /// 差分置き換えリクエストを送ると削除数とテキストを保持できる
    #[test]
    fn replace_suffix_request_holds_delete_count_and_text() {
        let (tx, mut rx) = mpsc::unbounded_channel::<TextInputRequest>();
        let handle = TextInputWorkerHandle::new(tx);

        let receiver = handle.send_replace_suffix(2, "world".to_string());

        assert!(receiver.is_ok());
        let request = rx.try_recv().expect("request should be sent");
        match request {
            TextInputRequest::ReplaceSuffix {
                delete_count,
                text,
                mode,
                ..
            } => {
                assert_eq!(delete_count, 2);
                assert_eq!(text, "world");
                assert_eq!(mode, TextInputExecutionMode::Standalone);
            }
            TextInputRequest::TypeText { .. } | TextInputRequest::SelectRecentRange { .. } => {
                panic!("unexpected request")
            }
        }
    }

    /// 連続差分置き換えリクエストは継続モードで送信される
    #[test]
    fn continuous_replace_suffix_request_uses_continuous_mode() {
        let (tx, mut rx) = mpsc::unbounded_channel::<TextInputRequest>();
        let handle = TextInputWorkerHandle::new(tx);

        let receiver = handle.send_replace_suffix_continuous(2, "world".to_string());

        assert!(receiver.is_ok());
        let request = rx.try_recv().expect("request should be sent");
        match request {
            TextInputRequest::ReplaceSuffix {
                delete_count,
                text,
                mode,
                ..
            } => {
                assert_eq!(delete_count, 2);
                assert_eq!(text, "world");
                assert_eq!(mode, TextInputExecutionMode::Continuous);
            }
            TextInputRequest::TypeText { .. } | TextInputRequest::SelectRecentRange { .. } => {
                panic!("unexpected request")
            }
        }
    }

    /// 範囲選択リクエストは移動量と選択長を保持できる
    #[test]
    fn select_recent_range_request_holds_relative_selection_parameters() {
        let (tx, mut rx) = mpsc::unbounded_channel::<TextInputRequest>();
        let handle = TextInputWorkerHandle::new(tx);

        let receiver = handle.send_select_recent_range(2, 4);

        assert!(receiver.is_ok());
        let request = rx.try_recv().expect("request should be sent");
        match request {
            TextInputRequest::SelectRecentRange {
                trailing_char_count,
                char_count,
                ..
            } => {
                assert_eq!(trailing_char_count, 2);
                assert_eq!(char_count, 4);
            }
            TextInputRequest::TypeText { .. } | TextInputRequest::ReplaceSuffix { .. } => {
                panic!("unexpected request")
            }
        }
    }
}
