//! voice-inputd : 録音 ⇆ 転写 を管理する常駐プロセス（single-thread runtime）
use std::{
    error::Error,
    fs,
    sync::{Arc, Mutex},
};

use arboard::Clipboard;
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::{
    net::{UnixListener, UnixStream},
    sync::{Semaphore, mpsc, oneshot},
    task::{LocalSet, spawn_local},
    time::Duration,
};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use voice_input::{
    domain::recorder::Recorder,
    infrastructure::{
        audio::CpalAudioBackend,
        external::{
            clipboard::get_selected_text,
            openai::transcribe_audio,
            sound::{pause_apple_music, play_start_sound, play_stop_sound, resume_apple_music},
        },
    },
    ipc::{IpcCmd, IpcResp, SOCKET_PATH},
};

const DEFAULT_MAX_RECORD_SECS: u64 = 30; // ← 変更したい場合は VOICE_INPUT_MAX_SECS を設定

// ────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecState {
    Idle,
    Recording,
}

/// 状態 + タイムアウト用キャンセルチャネル
#[derive(Debug)]
struct RecCtx {
    state: RecState,
    cancel: Option<oneshot::Sender<()>>,
}

// ────────────────────────────────────────────────────────
// トップレベル：single-thread Tokio runtime
// ────────────────────────────────────────────────────────
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    // TODO env の扱いまとめる
    if let Ok(path) = std::env::var("VOICE_INPUT_ENV_PATH") {
        dotenvy::from_path(path).ok();
    } else {
        dotenvy::dotenv().ok(); // fallback
    }
    let local = LocalSet::new();
    local.run_until(async_main()).await
}

// ---- 実際の処理本体（`Send` 制約を受けない） ----
async fn async_main() -> Result<(), Box<dyn Error>> {
    let _ = fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("voice-inputd listening on {SOCKET_PATH}");

    let recorder = Arc::new(Recorder::new(CpalAudioBackend::default()));
    let ctx = Arc::new(Mutex::new(RecCtx {
        state: RecState::Idle,
        cancel: None,
    }));

    // 転写キュー
    let (tx, rx) = mpsc::unbounded_channel::<(String, bool)>();
    let sem = Arc::new(Semaphore::new(2));

    // ─── 転写ワーカー（ローカルタスク） ──────────────────────────
    {
        let worker_sem = sem.clone();
        let mut rx = rx;
        spawn_local(async move {
            while let Some((wav, paste)) = rx.recv().await {
                let permit = worker_sem.clone().acquire_owned().await.unwrap();
                spawn_local(async move {
                    let _ = handle_transcription(&wav, paste).await;
                    drop(permit);
                });
            }
        });
    }

    loop {
        let (stream, _) = listener.accept().await?;
        let rec = recorder.clone();
        let ctx2 = ctx.clone();
        let tx2 = tx.clone();
        spawn_local(async move {
            let _ = handle_client(stream, rec, ctx2, tx2).await;
        });
    }
}

// ────────────────────────────────────────────────────────
async fn handle_client(
    stream: UnixStream,
    recorder: Arc<Recorder<CpalAudioBackend>>,
    ctx: Arc<Mutex<RecCtx>>,
    tx: mpsc::UnboundedSender<(String, bool)>,
) -> Result<(), Box<dyn Error>> {
    let (r, w) = stream.into_split();
    let mut reader = FramedRead::new(r, LinesCodec::new());
    let mut writer = FramedWrite::new(w, LinesCodec::new());

    if let Some(Ok(line)) = reader.next().await {
        let cmd: IpcCmd = serde_json::from_str(&line)?;
        let resp = match cmd {
            IpcCmd::Start { paste, prompt } => {
                start_recording(recorder.clone(), &ctx, &tx, paste, prompt).await
            }
            IpcCmd::Stop => stop_recording(recorder.clone(), &ctx, &tx, true, None).await,
            IpcCmd::Toggle { paste, prompt } => {
                if ctx.lock().unwrap().state == RecState::Idle {
                    start_recording(recorder.clone(), &ctx, &tx, paste, prompt).await
                } else {
                    stop_recording(recorder.clone(), &ctx, &tx, paste, prompt).await
                }
            }
            IpcCmd::Status => Ok(IpcResp {
                ok: true,
                msg: format!("state={:?}", ctx.lock().unwrap().state),
            }),
        }
        .unwrap_or_else(|e| IpcResp {
            ok: false,
            msg: e.to_string(),
        });

        writer.send(serde_json::to_string(&resp)?).await?;
    }
    Ok(())
}

// ────────────────────── 録音制御 ──────────────────────
async fn start_recording(
    recorder: Arc<Recorder<CpalAudioBackend>>,
    ctx: &Arc<Mutex<RecCtx>>,
    tx: &mpsc::UnboundedSender<(String, bool)>,
    paste: bool,
    _prompt: Option<String>,
) -> Result<IpcResp, Box<dyn Error>> {
    let mut c = ctx.lock().unwrap();
    if c.state != RecState::Idle {
        return Err("already recording".into());
    }

    if pause_apple_music() {
        fs::write("/tmp/voice_input_music_was_playing", "").ok();
    }
    play_start_sound();
    recorder.start()?;
    c.state = RecState::Recording;

    // ---- 自動停止タイマー --------------------------------------------
    let max_secs = std::env::var("VOICE_INPUT_MAX_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAX_RECORD_SECS);

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    c.cancel = Some(cancel_tx);
    drop(c); // lock を早めに解放

    let ctx_clone = ctx.clone();
    let tx_clone = tx.clone();
    spawn_local(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(max_secs)) => {
                if recorder.is_recording() {
                    if let Ok(wav) = recorder.stop() {
                        {
                            let mut c = ctx_clone.lock().unwrap();
                            c.state = RecState::Idle;
                            c.cancel = None;
                        }
                        let _ = tx_clone.send((wav, paste));
                        play_stop_sound();
                    }
                }
            }
            _ = cancel_rx => { /* 手動停止でキャンセル */ }
        }
    });

    Ok(IpcResp {
        ok: true,
        msg: format!("recording started (auto-stop in {max_secs}s)"),
    })
}

async fn stop_recording(
    recorder: Arc<Recorder<CpalAudioBackend>>,
    ctx: &Arc<Mutex<RecCtx>>,
    tx: &mpsc::UnboundedSender<(String, bool)>,
    paste: bool,
    prompt: Option<String>,
) -> Result<IpcResp, Box<dyn Error>> {
    let mut c = ctx.lock().unwrap();
    if c.state != RecState::Recording {
        return Err("not recording".into());
    }

    // タイマーをキャンセル
    if let Some(cancel) = c.cancel.take() {
        let _ = cancel.send(());
    }

    play_stop_sound();
    let wav = recorder.stop()?;
    c.state = RecState::Idle;

    if let Some(p) = prompt.or_else(|| get_selected_text().ok()) {
        let meta = format!("{wav}.json");
        fs::write(&meta, json!({ "prompt": p }).to_string())?;
    }
    tx.send((wav, paste))?;

    Ok(IpcResp {
        ok: true,
        msg: "recording stopped; queued".into(),
    })
}

// ────────────────────── 転写 & ペースト ─────────────────────
async fn handle_transcription(wav: &str, paste: bool) -> Result<(), Box<dyn Error>> {
    let text = transcribe_audio(wav, None).await?;

    // クリップボードへセット
    if let Err(e) = set_clipboard(&text).await {
        eprintln!("clipboard error: {e}");
    }

    // 即貼り付け (⌘V)
    if paste {
        tokio::time::sleep(Duration::from_millis(80)).await;
        tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "System Events" to keystroke "v" using {command down}"#)
            .output()
            .await
            .ok();
    }

    resume_apple_music();
    Ok(())
}

async fn set_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    if let Ok(mut cb) = Clipboard::new() {
        if cb.set_text(text).is_ok() {
            return Ok(());
        }
    }
    let mut child = tokio::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    use tokio::io::AsyncWriteExt;
    child
        .stdin
        .as_mut()
        .ok_or("failed to open pbcopy stdin")?
        .write_all(text.as_bytes())
        .await?;
    child.wait().await?;
    Ok(())
}
