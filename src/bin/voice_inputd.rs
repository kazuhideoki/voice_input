//! voice-inputd: 録音・転写を統括する常駐プロセス（シングルスレッド Tokio ランタイム）
//!
//! # 概要
//! CLI から Unix Domain Socket (UDS) 経由で受け取ったコマンドをハンドリングし、
//!  - `Recorder` を介した録音の開始 / 停止
//!  - OpenAI API を用いた文字起こし
//!  - クリップボードへの貼り付け & Apple Music の自動ポーズ / 再開
//! を非同期・協調的に実行します。
//!
//! *ソケットパス*: `/tmp/voice_input.sock`
//!
//! ## 実行モデル
//! - `tokio::main(flavor = "current_thread")` でシングルスレッドランタイムを起動
//! - クライアントごとの処理／転写ジョブは `spawn_local` でローカルタスク化
//! - 最大同時転写数を `Semaphore` で制御

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

/// デフォルトの最大録音秒数 (`VOICE_INPUT_MAX_SECS` が未設定の場合に適用)。
pub const DEFAULT_MAX_RECORD_SECS: u64 = 30;

// ────────────────────────────────────────────────────────

/// 録音ステートマシン。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecState {
    /// 録音していない待機状態
    Idle,
    /// 録音中
    Recording,
}

/// 録音状態とタイムアウトキャンセルチャネルをまとめた構造体。
#[derive(Debug)]
struct RecCtx {
    state: RecState,
    /// 自動停止タイマーのキャンセル用
    cancel: Option<oneshot::Sender<()>>,
}

// ────────────────────────────────────────────────────────
// エントリポイント： single‑thread Tokio runtime
// ────────────────────────────────────────────────────────

/// エントリポイント。環境変数を読み込み、`async_main` を current‑thread ランタイムで実行します。
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    // TODO env の扱いまとめる
    // .env 読み込み (VOICE_INPUT_ENV_PATH > .env)
    if let Ok(path) = std::env::var("VOICE_INPUT_ENV_PATH") {
        dotenvy::from_path(path).ok();
    } else {
        dotenvy::dotenv().ok();
    }

    let local = LocalSet::new();
    local.run_until(async_main()).await
}

/// ソケット待受・クライアントハンドリング・転写ワーカーを起動する本体。
async fn async_main() -> Result<(), Box<dyn Error>> {
    // 既存ソケットがあれば削除して再バインド
    let _ = fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("voice-inputd listening on {SOCKET_PATH}");

    let recorder = Arc::new(Recorder::new(CpalAudioBackend::default()));
    let ctx = Arc::new(Mutex::new(RecCtx {
        state: RecState::Idle,
        cancel: None,
    }));

    // 転写ジョブ用チャンネルと同時実行セマフォ
    let (tx, rx) = mpsc::unbounded_channel::<(String, bool)>();
    let sem = Arc::new(Semaphore::new(2));

    // ─── 転写ワーカー ─────────────────────────────
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

    // ─── クライアント接続ループ ──────────────────────
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
// クライアントハンドラ
// ────────────────────────────────────────────────────────

/// 1 クライアントとの IPC セッションを処理します。
/// CLI からの JSON 文字列を `IpcCmd` にデシリアライズし、
/// 状態とレコーダを操作して `IpcResp` を返送します。
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

/// 録音を開始し、自動停止タイマーを登録します。
///
/// * `paste` – 転写完了後に ⌘V ペーストを行うか
/// * `_prompt` – 将来の転写 API で使用するプロンプト (現状未使用)
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

    // Apple Music を一時停止し、録音開始 SE を再生
    if pause_apple_music() {
        let _ = fs::write("/tmp/voice_input_music_was_playing", "");
    }
    play_start_sound();

    recorder.start()?;
    c.state = RecState::Recording;

    // ---- 自動停止タイマー -----------------------------
    let max_secs = std::env::var("VOICE_INPUT_MAX_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAX_RECORD_SECS);

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    c.cancel = Some(cancel_tx);
    drop(c); // コンテキストロックを解放

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
            _ = cancel_rx => {
                // 手動停止によるキャンセル
            }
        }
    });

    Ok(IpcResp {
        ok: true,
        msg: format!("recording started (auto-stop in {max_secs}s)"),
    })
}

/// 録音停止処理。WAV を保存して転写キューに送信します。
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

    // 自動停止タイマーをキャンセル
    if let Some(cancel) = c.cancel.take() {
        let _ = cancel.send(());
    }

    play_stop_sound();
    let wav = recorder.stop()?;
    c.state = RecState::Idle;

    // 選択テキスト or 引数プロンプトをメタデータとしてJSON保存
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

/// WAV ファイルを OpenAI STT API で文字起こしし、結果をクリップボードへ保存。
/// `paste` フラグが `true` の場合は 80ms 後に ⌘V を送信して即貼り付けを行います。
async fn handle_transcription(wav: &str, paste: bool) -> Result<(), Box<dyn Error>> {
    let text = transcribe_audio(wav, None).await?;

    // クリップボードへコピー
    if let Err(e) = set_clipboard(&text).await {
        eprintln!("clipboard error: {e}");
    }

    // 即貼り付け
    if paste {
        tokio::time::sleep(Duration::from_millis(80)).await;
        let _ = tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application \"System Events\" to keystroke \"v\" using {command down}"#)
            .output()
            .await;
    }

    resume_apple_music();
    Ok(())
}

/// クリップボード (arboard→pbcopy フォールバック) にテキストを設定します。
async fn set_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    if let Ok(mut cb) = Clipboard::new() {
        if cb.set_text(text).is_ok() {
            return Ok(());
        }
    }
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or("failed to open pbcopy stdin")?
        .write_all(text.as_bytes())
        .await?;
    child.wait().await?;
    Ok(())
}
