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
    sync::{Semaphore, mpsc},
    task::{LocalSet, spawn_local},
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecState {
    Idle,
    Recording,
}

// ───────────────────────────────────────────────────────────
// single-thread Tokio runtime
// ───────────────────────────────────────────────────────────
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let local = LocalSet::new();
    local.run_until(async_main()).await
}

// ---- 実際の処理本体（`Send` 制約を受けない） ----
async fn async_main() -> Result<(), Box<dyn Error>> {
    let _ = fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("voice-inputd listening on {SOCKET_PATH}");

    let recorder = Arc::new(Recorder::new(CpalAudioBackend::default()));
    let state = Arc::new(Mutex::new(RecState::Idle));

    // 転写キュー
    let (tx, mut rx) = mpsc::unbounded_channel::<(String, bool)>();
    let sem = Arc::new(Semaphore::new(2));

    // ─── 転写ワーカー（ローカルタスク） ──────────────────────────
    {
        // Arc を clone して ‘static に
        let worker_sem = sem.clone();
        // ✅ rx をムーブして 'static に
        let mut rx = rx;
        spawn_local(async move {
            while let Some((wav, paste)) = rx.recv().await {
                // ✅ 所有権付き Permit に変更
                let permit = worker_sem.clone().acquire_owned().await.unwrap();

                spawn_local(async move {
                    let _ = handle_transcription(&wav, paste).await;
                    drop(permit); // ownedなので drop で OK
                });
            }
        });
    }

    loop {
        let (stream, _) = listener.accept().await?;
        let rec = recorder.clone();
        let st = state.clone();
        let tx2 = tx.clone();
        spawn_local(async move {
            let _ = handle_client(stream, rec, st, tx2).await;
        });
    }
}

// ───────────────────────────────────────────
async fn handle_client(
    stream: UnixStream,
    recorder: Arc<Recorder<CpalAudioBackend>>,
    state: Arc<Mutex<RecState>>,
    tx: mpsc::UnboundedSender<(String, bool)>,
) -> Result<(), Box<dyn Error>> {
    let (r, w) = stream.into_split();
    let mut reader = FramedRead::new(r, LinesCodec::new());
    let mut writer = FramedWrite::new(w, LinesCodec::new());

    if let Some(Ok(line)) = reader.next().await {
        let cmd: IpcCmd = serde_json::from_str(&line)?;
        let resp = match cmd {
            IpcCmd::Start { paste, prompt } => {
                start_recording(&recorder, &state, paste, prompt).await
            }
            IpcCmd::Stop => stop_recording(&recorder, &state, &tx, true, None).await,
            IpcCmd::Toggle { paste, prompt } => {
                if *state.lock().unwrap() == RecState::Idle {
                    start_recording(&recorder, &state, paste, prompt).await
                } else {
                    stop_recording(&recorder, &state, &tx, paste, prompt).await
                }
            }
            IpcCmd::Status => Ok(IpcResp {
                ok: true,
                msg: format!("state={:?}", *state.lock().unwrap()),
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
    recorder: &Recorder<CpalAudioBackend>,
    state: &Arc<Mutex<RecState>>,
    _paste: bool,
    _prompt: Option<String>,
) -> Result<IpcResp, Box<dyn Error>> {
    let mut st = state.lock().unwrap();
    if *st != RecState::Idle {
        return Err("already recording".into());
    }

    if pause_apple_music() {
        fs::write("/tmp/voice_input_music_was_playing", "").ok();
    }
    play_start_sound();
    recorder.start()?;
    *st = RecState::Recording;

    Ok(IpcResp {
        ok: true,
        msg: "recording started".into(),
    })
}

async fn stop_recording(
    recorder: &Recorder<CpalAudioBackend>,
    state: &Arc<Mutex<RecState>>,
    tx: &mpsc::UnboundedSender<(String, bool)>,
    paste: bool,
    prompt: Option<String>,
) -> Result<IpcResp, Box<dyn Error>> {
    let mut st = state.lock().unwrap();
    if *st != RecState::Recording {
        return Err("not recording".into());
    }

    play_stop_sound();
    let wav = recorder.stop()?;
    *st = RecState::Idle;

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

// ────────────────────── 転写 & ペースト ──────────────────────
async fn handle_transcription(wav: &str, paste: bool) -> Result<(), Box<dyn Error>> {
    let text = transcribe_audio(wav, None).await?;

    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(&text)?;

    if paste {
        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;
        tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "System Events" to keystroke "v" using {command down}"#)
            .output()
            .await
            .ok();
    }

    if std::path::Path::new("/tmp/voice_input_music_was_playing").exists() {
        resume_apple_music();
        let _ = fs::remove_file("/tmp/voice_input_music_was_playing");
    }
    Ok(())
}
