//! 常駐ワーカー向けのテキスト入力インターフェース定義
//!
//! enigo を同一プロセスの別スレッドで常駐させる前提の型を提供する。

use async_trait::async_trait;
use enigo::{
    Direction::{Click, Release},
    Enigo, Key, Keyboard, Settings,
};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

use crate::error::VoiceInputError;
use crate::utils::diagnostics;
use serde_json::json;

// Enigo/macOS の1イベント上限より小さくし、UTF-16の長さでも上限を守る。
const INPUT_CHUNK_UTF16_LIMIT: usize = 16;
// OSと入力先がイベントを処理できるよう、継続入力と削除にも間隔を設ける。
const INPUT_EVENT_INTERVAL: Duration = Duration::from_millis(20);

/// 入力要求の識別子と、キューへ投入した時刻。
#[derive(Debug)]
pub struct InputRequestTrace {
    id: u64,
    queued_at: Instant,
}

impl InputRequestTrace {
    fn new() -> Self {
        Self {
            id: diagnostics::next_id(),
            queued_at: Instant::now(),
        }
    }
}

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

impl From<TextInputWorkerError> for VoiceInputError {
    fn from(error: TextInputWorkerError) -> Self {
        match error {
            TextInputWorkerError::EnigoInitFailed(msg) => {
                VoiceInputError::TextInputWorkerInitFailed(msg)
            }
            TextInputWorkerError::WorkerSpawnFailed(msg) => {
                VoiceInputError::TextInputWorkerInitFailed(msg)
            }
            TextInputWorkerError::InputFailed(msg) => {
                VoiceInputError::TextInputWorkerInputFailed(msg)
            }
            TextInputWorkerError::ChannelClosed(msg) => {
                VoiceInputError::TextInputWorkerChannelClosed(msg)
            }
        }
    }
}

impl TextInputWorkerError {
    fn kind(&self) -> &'static str {
        match self {
            Self::EnigoInitFailed(_) => "enigo_init_failed",
            Self::WorkerSpawnFailed(_) => "worker_spawn_failed",
            Self::InputFailed(_) => "input_failed",
            Self::ChannelClosed(_) => "channel_closed",
        }
    }
}
/// ワーカーへ送る入力リクエスト
#[derive(Debug)]
pub enum TextInputRequest {
    /// テキストをそのまま入力
    TypeText {
        /// キュー待機時間を計測するためのメタデータ。
        trace: InputRequestTrace,
        /// 入力するテキスト
        text: String,
        /// 入力実行モード
        mode: TextInputExecutionMode,
        /// 完了通知用のチャネル
        completion: oneshot::Sender<Result<(), TextInputWorkerError>>,
    },
    /// 入力済みテキストの末尾を削って差分を入力
    ReplaceSuffix {
        /// キュー待機時間を計測するためのメタデータ。
        trace: InputRequestTrace,
        /// 削除する文字数
        delete_count: usize,
        /// 追加するテキスト
        text: String,
        /// 入力実行モード
        mode: TextInputExecutionMode,
        /// 完了通知用のチャネル
        completion: oneshot::Sender<Result<(), TextInputWorkerError>>,
    },
}

impl TextInputRequest {
    /// 完了通知用のチャネル
    fn completion(self) -> oneshot::Sender<Result<(), TextInputWorkerError>> {
        match self {
            TextInputRequest::TypeText { completion, .. }
            | TextInputRequest::ReplaceSuffix { completion, .. } => completion,
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
}

/// ワーカーへの送信ハンドル
#[derive(Clone)]
pub struct TextInputWorkerHandle {
    sender: mpsc::UnboundedSender<TextInputRequest>,
}

impl TextInputWorkerHandle {
    /// 入力せずに、ワーカーがリクエストを受け取れるかを確認する。
    pub fn is_running(&self) -> bool {
        !self.sender.is_closed()
    }

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
        let trace = InputRequestTrace::new();
        let request_id = trace.id;
        diagnostics::log(
            "input.queued",
            json!({"request_id": trace.id, "chars": text.chars().count(), "delete_chars": 0}),
        );
        self.sender
            .send(TextInputRequest::TypeText {
                trace,
                text,
                mode,
                completion: tx,
            })
            .map_err(|e| {
                diagnostics::log("input.enqueue_failed", json!({"request_id": request_id}));
                TextInputWorkerError::ChannelClosed(format!("send failed: {}", e))
            })?;
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
        let trace = InputRequestTrace::new();
        let request_id = trace.id;
        diagnostics::log(
            "input.queued",
            json!({"request_id": trace.id, "chars": text.chars().count(), "delete_chars": delete_count}),
        );
        self.sender
            .send(TextInputRequest::ReplaceSuffix {
                trace,
                delete_count,
                text,
                mode,
                completion: tx,
            })
            .map_err(|e| {
                diagnostics::log("input.enqueue_failed", json!({"request_id": request_id}));
                TextInputWorkerError::ChannelClosed(format!("send failed: {}", e))
            })?;
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
            diagnostics::log("input.init_failed", json!({"reason": "enigo_init"}));
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
        let (trace, text, mode, delete_count, completion) = match req {
            TextInputRequest::TypeText {
                trace,
                text,
                mode,
                completion,
            } => (trace, text, mode, 0, completion),
            TextInputRequest::ReplaceSuffix {
                trace,
                delete_count,
                text,
                mode,
                completion,
            } => (trace, text, mode, delete_count, completion),
        };
        let queue_ms = trace.queued_at.elapsed().as_millis();
        let started = Instant::now();
        diagnostics::log(
            "input.begin",
            json!({
                "request_id": trace.id, "queue_ms": queue_ms, "queue_depth": rx.len(),
                "chars": text.chars().count(), "delete_chars": delete_count,
            }),
        );
        let result = replace_suffix_with_enigo(&mut enigo, delete_count, &text, mode);
        diagnostics::log(
            "input.end",
            json!({
                "request_id": trace.id, "elapsed_ms": started.elapsed().as_millis(),
                "ok": result.is_ok(), "delivery": if result.is_ok() { "posted_unverified" } else { "failed" },
                "error_kind": result.as_ref().err().map(TextInputWorkerError::kind),
            }),
        );
        if completion.send(result).is_err() {
            diagnostics::log("input.completion_dropped", json!({"request_id": trace.id}));
        }
    }
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

    apply_paced_input(
        delete_count,
        text,
        |step| {
            match step {
                InputStep::Delete => enigo.key(Key::Backspace, Click),
                InputStep::Text(chunk) => enigo.text(chunk),
            }
            .map_err(|e| TextInputWorkerError::InputFailed(e.to_string()))
        },
        std::thread::sleep,
    )
}

fn prepare_input(enigo: &mut Enigo) -> Result<(), TextInputWorkerError> {
    std::thread::sleep(std::time::Duration::from_millis(50));
    enigo
        .key(Key::Meta, Release)
        .map_err(|e| TextInputWorkerError::InputFailed(e.to_string()))?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum InputStep<'a> {
    Delete,
    Text(&'a str),
}

fn apply_paced_input(
    delete_count: usize,
    text: &str,
    mut emit: impl FnMut(InputStep<'_>) -> Result<(), TextInputWorkerError>,
    mut wait: impl FnMut(Duration),
) -> Result<(), TextInputWorkerError> {
    for _ in 0..delete_count {
        emit(InputStep::Delete)?;
        wait(INPUT_EVENT_INTERVAL);
    }
    let mut start = 0;
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units + ch.len_utf16() > INPUT_CHUNK_UTF16_LIMIT {
            emit(InputStep::Text(&text[start..index]))?;
            wait(INPUT_EVENT_INTERVAL);
            start = index;
            units = 0;
        }
        units += ch.len_utf16();
    }
    if start < text.len() {
        emit(InputStep::Text(&text[start..]))?;
        // 次の要求（特に最終補正の削除）が先行しないよう末尾でも待機する。
        wait(INPUT_EVENT_INTERVAL);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 日本語と絵文字を欠落させずUTF-16上限内で分割し、各送信後に待機する
    #[test]
    fn paced_input_preserves_unicode_and_waits_after_every_chunk() {
        let text = "あいうえお😀かきくけこ😸さしすせそたちつてと";
        let steps = std::cell::RefCell::new(Vec::new());
        let mut emitted = String::new();
        apply_paced_input(
            0,
            text,
            |step| {
                let InputStep::Text(chunk) = step else {
                    panic!("unexpected deletion")
                };
                assert!(chunk.encode_utf16().count() <= INPUT_CHUNK_UTF16_LIMIT);
                emitted.push_str(chunk);
                steps.borrow_mut().push("emit");
                Ok(())
            },
            |duration| {
                assert_eq!(duration, INPUT_EVENT_INTERVAL);
                steps.borrow_mut().push("wait");
            },
        )
        .unwrap();
        assert_eq!(emitted, text);
        assert!(steps.borrow().len() > 2);
        assert!(
            steps
                .borrow()
                .chunks_exact(2)
                .all(|pair| pair == ["emit", "wait"])
        );
    }

    /// 削除だけの最終補正でも各Backspaceの後で待機する
    #[test]
    fn deletion_only_patch_is_paced() {
        let steps = std::cell::RefCell::new(Vec::new());
        apply_paced_input(
            3,
            "",
            |step| {
                assert_eq!(step, InputStep::Delete);
                steps.borrow_mut().push("delete");
                Ok(())
            },
            |_| steps.borrow_mut().push("wait"),
        )
        .unwrap();
        assert_eq!(
            *steps.borrow(),
            ["delete", "wait", "delete", "wait", "delete", "wait"]
        );
    }

    /// 入力途中の失敗後は後続文字も削除も送信しない
    #[test]
    fn failed_emission_stops_patch_without_retrying() {
        let mut calls = 0;
        let mut waits = 0;
        let result = apply_paced_input(
            2,
            "追加",
            |_| {
                calls += 1;
                if calls == 2 {
                    return Err(TextInputWorkerError::InputFailed("busy".into()));
                }
                Ok(())
            },
            |_| waits += 1,
        );
        assert!(matches!(result, Err(TextInputWorkerError::InputFailed(_))));
        assert_eq!(calls, 2);
        assert_eq!(waits, 1);
    }

    /// 送信が遅くても文字列を再送せず削除と追記の順序を保つ
    #[test]
    fn slow_emission_preserves_patch_order_without_duplicates() {
        let mut steps = Vec::new();
        apply_paced_input(
            2,
            "訂正",
            |step| {
                std::thread::sleep(Duration::from_millis(5));
                steps.push(match step {
                    InputStep::Delete => "delete".to_string(),
                    InputStep::Text(text) => text.to_string(),
                });
                Ok(())
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(steps, ["delete", "delete", "訂正"]);
    }

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
            TextInputRequest::ReplaceSuffix { .. } => {
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
            TextInputRequest::ReplaceSuffix { .. } => {
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
            TextInputRequest::TypeText { .. } => {
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
            TextInputRequest::TypeText { .. } => {
                panic!("unexpected request")
            }
        }
    }
}
