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
    time::{Duration, SystemTime},
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
        audio::CpalAudioBackend,
        command_handler::CommandHandler,
        external::text_input,
        push_to_talk::{self, PushToTalkEvent, PushToTalkMonitor},
        runtime_recovery::{SleepWakeDetector, WakeRecoveryRetryPolicy},
        service_container::ServiceContainer,
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
    let history_service = container.history_service.clone();

    text_input::init_worker().map_err(|e| VoiceInputError::SystemError(e.to_string()))?;
    spawn_runtime_recovery_monitor(recording_service.clone(), command_handler.clone());
    command_handler
        .borrow()
        .warm_realtime_whisper_session_if_enabled();
    let _push_to_talk_monitor = match spawn_push_to_talk_if_enabled(
        command_handler.clone(),
        recording_service.clone(),
        &EnvConfig::get().push_to_talk,
    ) {
        Ok(monitor) => monitor,
        Err(error) => {
            eprintln!("Push-to-talk initialization failed: {error}");
            eprintln!(
                "voice-inputd is stopping without restart to avoid a LaunchAgent restart loop."
            );
            eprintln!(
                "Grant Accessibility/Input Monitoring permission to VoiceInput.app, or set VOICE_INPUT_PUSH_TO_TALK=false, then restart the daemon."
            );
            let _ = fs::remove_file(&path);
            process::exit(0);
        }
    };

    spawn_local(spawn_transcription_worker(
        semaphore.clone(),
        transcription_rx,
        transcription_service,
        history_service,
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

fn spawn_push_to_talk_if_enabled(
    command_handler: std::rc::Rc<std::cell::RefCell<CommandHandler<CpalAudioBackend>>>,
    recording_service: std::rc::Rc<
        std::cell::RefCell<voice_input::application::RecordingService<CpalAudioBackend>>,
    >,
    config: &voice_input::utils::config::PushToTalkConfig,
) -> Result<Option<PushToTalkMonitor>> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let monitor = push_to_talk::spawn_monitor(config, tx)?;
    if monitor.is_none() {
        return Ok(None);
    }

    println!("push-to-talk enabled: hotkey={}", config.hotkey);
    spawn_local(async move {
        let mut push_to_talk_recording = false;

        while let Some(event) = rx.recv().await {
            match event {
                PushToTalkEvent::KeyDown => {
                    if push_to_talk_recording || recording_service.borrow().is_recording() {
                        continue;
                    }

                    let response = command_handler
                        .borrow()
                        .handle(IpcCmd::Start {
                            prompt: None,
                            save_audio_path: None,
                            max_duration_secs: None,
                            transcription_provider: None,
                            transcription_model: None,
                        })
                        .await;

                    match response {
                        Ok(_) => push_to_talk_recording = true,
                        Err(error) => eprintln!("Push-to-talk start failed: {error}"),
                    }
                }
                PushToTalkEvent::KeyUp => {
                    if !push_to_talk_recording {
                        continue;
                    }

                    push_to_talk_recording = false;
                    if !recording_service.borrow().is_recording() {
                        continue;
                    }

                    if let Err(error) = command_handler.borrow().handle(IpcCmd::Stop).await {
                        eprintln!("Push-to-talk stop failed: {error}");
                    }
                }
            }
        }
    });

    Ok(monitor)
}

fn spawn_runtime_recovery_monitor(
    recording_service: std::rc::Rc<
        std::cell::RefCell<voice_input::application::RecordingService<CpalAudioBackend>>,
    >,
    command_handler: std::rc::Rc<std::cell::RefCell<CommandHandler<CpalAudioBackend>>>,
) {
    const CHECK_INTERVAL: Duration = Duration::from_secs(15);
    const WAKE_THRESHOLD: Duration = Duration::from_secs(45);

    spawn_local(async move {
        let mut detector = SleepWakeDetector::new(SystemTime::now(), WAKE_THRESHOLD);
        let retry_policy = WakeRecoveryRetryPolicy::after_wake();
        let mut ticker = tokio::time::interval(CHECK_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            if !detector.record_tick(SystemTime::now()) {
                continue;
            }

            if recording_service.borrow().is_recording() {
                eprintln!("Wake detected while recording; deferred runtime recovery.");
                continue;
            }

            let mut recovered = false;
            for attempt in 1..=retry_policy.max_attempts {
                let audio_result = recording_service.borrow().recover_after_wake();
                let text_result = text_input::recover_after_wake()
                    .map_err(|e| VoiceInputError::SystemError(e.to_string()));

                match (audio_result, text_result) {
                    (Ok(()), Ok(())) => {
                        recovered = true;
                        command_handler
                            .borrow()
                            .reset_ready_realtime_whisper_session();
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

                tokio::time::sleep(retry_policy.retry_interval).await;
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
