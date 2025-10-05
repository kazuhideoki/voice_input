//! voice-inputd: 録音・転写を統括する常駐プロセス（シングルスレッド Tokio ランタイム）
//!
//! # 概要
//! CLI から Unix Domain Socket (UDS) 経由で受け取ったコマンドをハンドリングし、
//!  - 録音の開始 / 停止
//!  - OpenAI API を用いた文字起こし
//!  - クリップボードへの貼り付け & Apple Music の自動ポーズ / 再開
//!    を非同期・協調的に実行します。
//!
//! *ソケットパス*: `/tmp/voice_input.sock`

#![allow(clippy::await_holding_refcell_ref)]

use std::{error::Error, fs};

use futures::{SinkExt, StreamExt};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::Semaphore,
    task::{LocalSet, spawn_local},
};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use voice_input::{
    application::{ServiceContainer, spawn_transcription_worker},
    error::{Result, VoiceInputError},
    infrastructure::audio::CpalAudioBackend,
    ipc::{IpcCmd, IpcResp, socket_path},
    load_env,
    utils::config::EnvConfig,
};

// ────────────────────────────────────────────────────────
// エントリポイント： single‑thread Tokio runtime
// ────────────────────────────────────────────────────────

/// エントリポイント。環境変数を読み込み、`async_main` を current‑thread ランタイムで実行します。
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::result::Result<(), Box<dyn Error>> {
    load_env();

    // 環境変数設定を初期化
    EnvConfig::init().map_err(|e| VoiceInputError::ConfigInitError(e.to_string()))?;

    // `spawn_local` はこのスレッドだけで動かしたい非同期ジョブを登録する。LocalSet はその実行エンジン
    let local = LocalSet::new();
    local
        .run_until(async_main())
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error>)
}

/// ソケット待受・クライアントハンドリング・転写ワーカーを起動する本体。
async fn async_main() -> Result<()> {
    // 既存ソケットがあれば削除して再バインド
    let path = socket_path();
    let _ = fs::remove_file(&path);
    let listener = UnixListener::bind(&path)
        .map_err(|e| VoiceInputError::IpcConnectionFailed(e.to_string()))?;
    println!("voice-inputd listening on {:?}", path);

    // サービスコンテナを初期化
    let mut container = ServiceContainer::<CpalAudioBackend>::new()?;
    let command_handler = container.command_handler.clone();
    let transcription_rx = container
        .take_transcription_rx()
        .expect("Transcription receiver should be available");

    // 転写ワーカーの起動
    let semaphore = std::sync::Arc::new(Semaphore::new(2));
    let transcription_service = {
        // TranscriptionServiceを取得（CommandHandlerから）
        // 注: 実際のアプリケーションではServiceContainerから直接取得する方が良い
        use voice_input::application::TranscriptionService;
        use voice_input::infrastructure::external::openai_adapter::OpenAiTranscriptionAdapter;
        std::rc::Rc::new(std::cell::RefCell::new(
            TranscriptionService::with_default_repo(Box::new(OpenAiTranscriptionAdapter::new()?)),
        ))
    };

    spawn_local(spawn_transcription_worker(
        semaphore.clone(),
        transcription_rx,
        transcription_service,
    ));

    // クライアント接続ループ
    loop {
        let (stream, _) = listener.accept().await?;
        let handler = command_handler.clone();
        spawn_local(async move {
            let _ = handle_client(stream, handler).await;
        });
    }
}

/// 1 クライアントとの IPC セッションを処理します。
async fn handle_client(
    stream: UnixStream,
    command_handler: std::rc::Rc<
        std::cell::RefCell<voice_input::application::CommandHandler<CpalAudioBackend>>,
    >,
) -> Result<()> {
    let (r, w) = stream.into_split();
    let mut reader = FramedRead::new(r, LinesCodec::new());
    let mut writer = FramedWrite::new(w, LinesCodec::new());

    if let Some(Ok(line)) = reader.next().await {
        let cmd: IpcCmd = serde_json::from_str(&line)
            .map_err(|e| VoiceInputError::IpcSerializationError(e.to_string()))?;

        let resp = command_handler
            .borrow()
            .handle(cmd)
            .await
            .unwrap_or_else(|e| IpcResp {
                ok: false,
                msg: e.to_string(),
            });

        writer
            .send(
                serde_json::to_string(&resp)
                    .map_err(|e| VoiceInputError::IpcSerializationError(e.to_string()))?,
            )
            .await
            .map_err(|e| VoiceInputError::IpcConnectionFailed(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "Requires audio device"]
    async fn test_daemon_initialization() -> Result<()> {
        // サービスコンテナが正しく初期化されることを確認
        let container = ServiceContainer::<CpalAudioBackend>::new();

        assert!(container.is_ok());
        Ok(())
    }
}
