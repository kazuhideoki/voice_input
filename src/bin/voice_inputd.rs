//! voice-inputd: 録音・転写を統括する常駐プロセス（シングルスレッド Tokio ランタイム）
//!
//! # 概要
//! CLI から Unix Domain Socket (UDS) 経由で受け取ったコマンドをハンドリングし、
//!  - 録音の開始 / 停止
//!  - 設定済み転写バックエンドを用いた文字起こし
//!  - 直接入力 & Apple Music の自動ポーズ / 再開
//!    を非同期・協調的に実行します。
//!
//! *ソケットパス*: `/tmp/voice_input.sock`（環境変数で上書き可能）

#![allow(clippy::await_holding_refcell_ref)]

use std::{
    error::Error,
    fs, process,
    time::{Duration, Instant},
};

use futures::{SinkExt, StreamExt};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::Semaphore,
    task::{LocalSet, spawn_local},
};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use voice_input::{
    error::{Result, VoiceInputError},
    infrastructure::{
        audio::CpalAudioBackend, command_handler::CommandHandler, external::text_input,
        runtime_recovery::SleepWakeDetector, service_container::ServiceContainer,
        transcription_worker::spawn_transcription_worker,
    },
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
    let recording_service = container.recording_service.clone();
    let transcription_rx = container
        .take_transcription_rx()
        .expect("Transcription receiver should be available");

    // 転写ワーカーの起動
    let max_concurrent_transcriptions = EnvConfig::get().recommended_transcription_parallelism();
    let semaphore = std::sync::Arc::new(Semaphore::new(max_concurrent_transcriptions));
    let transcription_service = container.transcription_service.clone();

    text_input::init_worker().map_err(|e| VoiceInputError::SystemError(e.to_string()))?;
    spawn_runtime_recovery_monitor(recording_service.clone());

    spawn_local(spawn_transcription_worker(
        semaphore.clone(),
        transcription_rx,
        transcription_service,
        recording_service,
    ));

    // クライアント接続ループ
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| VoiceInputError::IpcConnectionFailed(e.to_string()))?;
        let handler = command_handler.clone();
        spawn_local(async move {
            let _ = handle_client(stream, handler).await;
        });
    }
}

fn spawn_runtime_recovery_monitor(
    recording_service: std::rc::Rc<
        std::cell::RefCell<voice_input::application::RecordingService<CpalAudioBackend>>,
    >,
) {
    const CHECK_INTERVAL: Duration = Duration::from_secs(15);
    const WAKE_THRESHOLD: Duration = Duration::from_secs(45);
    const RECOVERY_RETRY_INTERVAL: Duration = Duration::from_secs(2);
    const MAX_RECOVERY_ATTEMPTS: usize = 3;

    spawn_local(async move {
        let mut detector = SleepWakeDetector::new(Instant::now(), WAKE_THRESHOLD);
        let mut ticker = tokio::time::interval(CHECK_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            if !detector.record_tick(Instant::now()) {
                continue;
            }

            if recording_service.borrow().is_recording() {
                eprintln!("Wake detected while recording; deferred runtime recovery.");
                continue;
            }

            let mut recovered = false;
            for attempt in 1..=MAX_RECOVERY_ATTEMPTS {
                let audio_result = recording_service.borrow().recover_after_wake();
                let text_result = text_input::recover_after_wake()
                    .map_err(|e| VoiceInputError::SystemError(e.to_string()));

                match (audio_result, text_result) {
                    (Ok(()), Ok(())) => {
                        recovered = true;
                        println!("Recovered runtime resources after wake.");
                        break;
                    }
                    (audio_result, text_result) => {
                        if let Err(err) = audio_result {
                            eprintln!(
                                "Wake recovery attempt {} failed for audio backend: {}",
                                attempt, err
                            );
                        }
                        if let Err(err) = text_result {
                            eprintln!(
                                "Wake recovery attempt {} failed for text input worker: {}",
                                attempt, err
                            );
                        }
                    }
                }

                tokio::time::sleep(RECOVERY_RETRY_INTERVAL).await;
            }

            if recovered {
                continue;
            }

            eprintln!("Wake recovery failed; exiting to let LaunchAgent restart the daemon.");
            process::exit(75);
        }
    });
}

/// 1 クライアントとの IPC セッションを処理します。
async fn handle_client(
    stream: UnixStream,
    command_handler: std::rc::Rc<std::cell::RefCell<CommandHandler<CpalAudioBackend>>>,
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

    /// サービスコンテナが初期化できる
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "Requires audio device"]
    async fn daemon_initializes_service_container() -> Result<()> {
        // サービスコンテナが正しく初期化されることを確認
        let container = ServiceContainer::<CpalAudioBackend>::new();

        assert!(container.is_ok());
        Ok(())
    }
}
