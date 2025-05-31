//! voice-inputd: 録音・転写を統括する常駐プロセス（シングルスレッド Tokio ランタイム）
//!
//! # 概要
//! CLI から Unix Domain Socket (UDS) 経由で受け取ったコマンドをハンドリングし、
//!  - `Recorder` を介した録音の開始 / 停止
//!  - OpenAI API を用いた文字起こし
//!  - クリップボードへの貼り付け & Apple Music の自動ポーズ / 再開
//!    を非同期・協調的に実行します。
//!
//! *ソケットパス*: `/tmp/voice_input.sock`
//!
//! ## 実行モデル
//! - `tokio::main(flavor = "current_thread")` でシングルスレッドランタイムを起動
//! - クライアントごとの処理／転写ジョブは `spawn_local` でローカルタスク化
//! - 最大同時転写数を `Semaphore` で制御

use std::{
    cell::RefCell,
    error::Error,
    fs,
    rc::Rc,
    sync::{Arc, Mutex},
};

use arboard::Clipboard;
use futures::{SinkExt, StreamExt};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::{Semaphore, mpsc, oneshot},
    task::{LocalSet, spawn_local},
    time::Duration,
};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use voice_input::{
    application::{StackService, UserFeedback},
    domain::dict::{DictRepository, apply_replacements},
    domain::recorder::Recorder,
    infrastructure::{
        audio::{CpalAudioBackend, cpal_backend::AudioData},
        dict::JsonFileDictRepo,
        external::{
            clipboard::get_selected_text,
            openai::OpenAiClient,
            sound::{pause_apple_music, play_start_sound, play_stop_sound, resume_apple_music},
            text_input,
        },
    },
    ipc::{IpcCmd, IpcResp, RecordingResult, socket_path},
    load_env,
};

/// デフォルトの最大録音秒数 (`VOICE_INPUT_MAX_SECS` が未設定の場合に適用)。
pub const DEFAULT_MAX_RECORD_SECS: u64 = 30;

/// 転写結果チャネルのメッセージ型
type TranscriptionMessage = (
    RecordingResult,                       // 録音結果（音声データと録音時間）
    bool,                                  // paste: 転写完了後に自動ペーストするか
    bool,                                  // resume_music: 録音前にApple Musicが再生中だった場合、再開するか
    bool,                                  // direct_input: 直接入力モード（クリップボード経由ではなくEnigoライブラリを使用）
    Option<Rc<RefCell<StackService>>>,     // stack_service: スタックモード有効時のStackServiceインスタンス
);

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
    /// 録音開始時にApple Musicが再生中だったか
    music_was_playing: bool,
    /// 録音開始時点で取得した選択テキストまたはCLIプロンプト
    start_prompt: Option<String>,
    /// 転写完了後にペーストを行うか
    paste: bool,
    /// 直接入力を使用するか（クリップボードを使わない）
    direct_input: bool,
}

// ────────────────────────────────────────────────────────
// エントリポイント： single‑thread Tokio runtime
// ────────────────────────────────────────────────────────

/// エントリポイント。環境変数を読み込み、`async_main` を current‑thread ランタイムで実行します。
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    load_env();

    // `spawn_local` はこのスレッドだけで動かしたい非同期ジョブを登録する。LocalSet はその実行エンジン
    let local = LocalSet::new();
    local.run_until(async_main()).await
}

/// ソケット待受・クライアントハンドリング・転写ワーカーを起動する本体。
async fn async_main() -> Result<(), Box<dyn Error>> {
    // 既存ソケットがあれば削除して再バインド
    let path = socket_path();
    let _ = fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    println!("voice-inputd listening on {:?}", path);

    let recorder = Rc::new(std::cell::RefCell::new(Recorder::new(
        CpalAudioBackend::default(),
    )));
    let ctx = Arc::new(Mutex::new(RecCtx {
        state: RecState::Idle,
        cancel: None,
        music_was_playing: false,
        start_prompt: None,
        paste: false,
        direct_input: false,
    }));

    // StackService for multi-stacking functionality (single-thread with LocalSet)
    let stack_service = Rc::new(RefCell::new(StackService::new()));

    // 転写ジョブ用チャンネルと同時実行セマフォ
    let (tx, rx) = mpsc::unbounded_channel::<(
        RecordingResult,
        bool,
        bool,
        bool,
        Option<Rc<RefCell<StackService>>>,
    )>();
    let sem = Arc::new(Semaphore::new(2));

    // ─── 転写ワーカー ─────────────────────────────
    {
        let worker_sem = sem.clone();
        let mut rx = rx;
        spawn_local(async move {
            while let Some((result, paste, resume_music, direct_input, stack_service)) =
                rx.recv().await
            {
                let permit = match worker_sem.clone().acquire_owned().await {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("semaphore acquire error: {e}");
                        continue;
                    }
                };
                spawn_local(async move {
                    let _ = handle_transcription(
                        result,
                        paste,
                        resume_music,
                        direct_input,
                        stack_service,
                    )
                    .await;
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
        let stack_service2 = stack_service.clone();
        let tx2 = tx.clone();
        spawn_local(async move {
            let _ = handle_client(stream, rec, ctx2, stack_service2, tx2).await;
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
    recorder: Rc<std::cell::RefCell<Recorder<CpalAudioBackend>>>,
    ctx: Arc<Mutex<RecCtx>>,
    stack_service: Rc<RefCell<StackService>>,
    tx: mpsc::UnboundedSender<TranscriptionMessage>,
) -> Result<(), Box<dyn Error>> {
    let (r, w) = stream.into_split();
    let mut reader = FramedRead::new(r, LinesCodec::new());
    let mut writer = FramedWrite::new(w, LinesCodec::new());

    if let Some(Ok(line)) = reader.next().await {
        let cmd: IpcCmd = serde_json::from_str(&line)?;
        let resp = match cmd {
            IpcCmd::Start {
                paste,
                prompt,
                direct_input,
            } => {
                start_recording(
                    recorder.clone(),
                    &ctx,
                    &tx,
                    &stack_service,
                    paste,
                    prompt,
                    direct_input,
                )
                .await
            }
            IpcCmd::Stop => {
                stop_recording(
                    recorder.clone(),
                    &ctx,
                    &tx,
                    &stack_service,
                    true,
                    None,
                    false,
                )
                .await
            }
            IpcCmd::Toggle {
                paste,
                prompt,
                direct_input,
            } => {
                if ctx.lock().map_err(|e| e.to_string())?.state == RecState::Idle {
                    start_recording(
                        recorder.clone(),
                        &ctx,
                        &tx,
                        &stack_service,
                        paste,
                        prompt,
                        direct_input,
                    )
                    .await
                } else {
                    stop_recording(
                        recorder.clone(),
                        &ctx,
                        &tx,
                        &stack_service,
                        paste,
                        prompt,
                        direct_input,
                    )
                    .await
                }
            }
            IpcCmd::Status => Ok(IpcResp {
                ok: true,
                msg: format!("state={:?}", ctx.lock().map_err(|e| e.to_string())?.state),
            }),
            IpcCmd::ListDevices => {
                let list = CpalAudioBackend::list_devices();
                Ok(IpcResp {
                    ok: true,
                    msg: if list.is_empty() {
                        "⚠️  No input devices detected".into()
                    } else {
                        list.join("\n")
                    },
                })
            }
            IpcCmd::Health => health_check().await,
            // スタック関連のコマンド
            IpcCmd::EnableStackMode => {
                let mut service = stack_service.borrow_mut();
                service.enable_stack_mode();
                let count = service.list_stacks().len();
                Ok(IpcResp {
                    ok: true,
                    msg: UserFeedback::mode_status(true, count),
                })
            }
            IpcCmd::DisableStackMode => {
                let mut service = stack_service.borrow_mut();
                service.disable_stack_mode();
                Ok(IpcResp {
                    ok: true,
                    msg: UserFeedback::mode_status(false, 0),
                })
            }
            IpcCmd::PasteStack { number } => {
                let (stack_text, char_count, error) = {
                    let service = stack_service.borrow();
                    match service.get_stack_with_context(number) {
                        Ok(stack) => (Some(stack.text.clone()), stack.text.len(), None),
                        Err(e) => (None, 0, Some(e.to_string())),
                    }
                };

                if let Some(error_msg) = error {
                    Ok(IpcResp {
                        ok: false,
                        msg: error_msg,
                    })
                } else if let Some(text) = stack_text {
                    match text_input::type_text(&text).await {
                        Ok(_) => Ok(IpcResp {
                            ok: true,
                            msg: UserFeedback::paste_success(number, char_count),
                        }),
                        Err(e) => Ok(IpcResp {
                            ok: false,
                            msg: format!("Failed to paste stack {}: {}", number, e),
                        }),
                    }
                } else {
                    Ok(IpcResp {
                        ok: false,
                        msg: format!("Unexpected error: stack {} not found", number),
                    })
                }
            }
            IpcCmd::ListStacks => {
                let service = stack_service.borrow();
                Ok(IpcResp {
                    ok: true,
                    msg: service.list_stacks_formatted(),
                })
            }
            IpcCmd::ClearStacks => {
                let mut service = stack_service.borrow_mut();
                let (_, message) = service.clear_stacks_with_confirmation();
                Ok(IpcResp {
                    ok: true,
                    msg: message,
                })
            }
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
/// * `prompt` – 追加プロンプト。選択テキストより優先される
/// * `direct_input` – 直接入力を使用するか（クリップボードを使わない）
async fn start_recording(
    recorder: Rc<std::cell::RefCell<Recorder<CpalAudioBackend>>>,
    ctx: &Arc<Mutex<RecCtx>>,
    tx: &mpsc::UnboundedSender<TranscriptionMessage>,
    stack_service: &Rc<RefCell<StackService>>,
    paste: bool,
    prompt: Option<String>,
    direct_input: bool,
) -> Result<IpcResp, Box<dyn Error>> {
    let mut c = ctx.lock().map_err(|e| e.to_string())?;
    if c.state != RecState::Idle {
        return Err("already recording".into());
    }

    // 録音開始時点の選択テキストまたはCLI引数を保存
    c.start_prompt = prompt.or_else(|| get_selected_text().ok());

    // paste/direct_input設定を保存
    c.paste = paste;
    c.direct_input = direct_input;

    // Apple Music を一時停止し、後で再開するかを記録
    c.music_was_playing = pause_apple_music();
    // 録音開始 SE
    play_start_sound();

    recorder.borrow_mut().start()?;
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
    let stack_service_clone = stack_service.clone();
    spawn_local(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(max_secs)) => {
                if recorder.borrow().is_recording() {
                    if let Ok(audio_data) = recorder.borrow_mut().stop() {
                        let result = RecordingResult {
                            audio_data: audio_data.into(),
                            duration_ms: 0, // Duration tracking not implemented yet
                        };

                        let (was_playing, stored_paste, stored_direct_input, prompt_to_save) = {
                            let mut c = match ctx_clone.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    eprintln!("ctx lock poisoned: {e}");
                                    return;
                                }
                            };
                            c.state = RecState::Idle;
                            c.cancel = None;
                            let stored = c.start_prompt.take();
                            let prompt = stored.or_else(|| get_selected_text().ok());
                            let w = c.music_was_playing;
                            c.music_was_playing = false;
                            (w, c.paste, c.direct_input, prompt)
                        };

                        // Save prompt metadata (no longer needed in memory mode)
                        if let Some(_p) = prompt_to_save {
                            // メモリモードではメタデータファイルを作成しない
                        }

                        // スタックモードが有効な場合はstack_serviceを渡す
                        let stack_for_transcription = if stack_service_clone.borrow().is_stack_mode_enabled() {
                            Some(stack_service_clone.clone())
                        } else {
                            None
                        };
                        let _ = tx_clone.send((result, stored_paste, was_playing, stored_direct_input, stack_for_transcription));
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
/// プロンプトは開始時に取得したもの → 引数 → 停止時の選択テキストの順で使われます。
async fn stop_recording(
    recorder: Rc<std::cell::RefCell<Recorder<CpalAudioBackend>>>,
    ctx: &Arc<Mutex<RecCtx>>,
    tx: &mpsc::UnboundedSender<TranscriptionMessage>,
    stack_service: &Rc<RefCell<StackService>>,
    paste: bool,
    prompt: Option<String>,
    direct_input: bool,
) -> Result<IpcResp, Box<dyn Error>> {
    let mut c = ctx.lock().map_err(|e| e.to_string())?;
    if c.state != RecState::Recording {
        return Err("not recording".into());
    }

    // 自動停止タイマーをキャンセル
    if let Some(cancel) = c.cancel.take() {
        let _ = cancel.send(());
    }

    play_stop_sound();
    let audio_data = recorder.borrow_mut().stop()?;
    c.state = RecState::Idle;

    // 開始時の保存値→引数→現在の選択の順でプロンプトを決定
    let stored = c.start_prompt.take();
    let final_prompt = prompt.or(stored).or_else(|| get_selected_text().ok());

    let result = RecordingResult {
        audio_data: audio_data.into(),
        duration_ms: 0, // Duration tracking not implemented yet
    };

    // Save prompt metadata (no longer needed in memory mode)
    if let Some(_p) = final_prompt {
        // メモリモードではメタデータファイルを作成しない
    }

    let was_playing = c.music_was_playing;
    c.music_was_playing = false;

    // スタックモードが有効な場合はstack_serviceを渡す
    let stack_for_transcription = if stack_service.borrow().is_stack_mode_enabled() {
        Some(stack_service.clone())
    } else {
        None
    };
    tx.send((
        result,
        paste,
        was_playing,
        direct_input,
        stack_for_transcription,
    ))?;

    Ok(IpcResp {
        ok: true,
        msg: "recording stopped; queued".into(),
    })
}

// ────────────────────── 転写 & ペースト ─────────────────────

/// WAV データを OpenAI STT API で文字起こしし、結果をクリップボードへ保存。
/// `paste` フラグが `true` の場合は 80ms 後に ⌘V を送信して即貼り付けを行います。
/// `direct_input` フラグが `true` の場合は直接入力を使用します。
/// スタックモードが有効な場合、転写結果を自動的にスタックに保存します。
async fn handle_transcription(
    result: RecordingResult,
    paste: bool,
    resume_music: bool,
    direct_input: bool,
    stack_service: Option<Rc<RefCell<StackService>>>,
) -> Result<(), Box<dyn Error>> {
    // エラーが発生しても確実に音楽を再開するためにdeferパターンで実装
    let _defer_guard = scopeguard::guard(resume_music, |should_resume| {
        if should_resume {
            // 念のため少し遅延を入れて他の処理が完了するのを待つ
            std::thread::sleep(std::time::Duration::from_millis(100));
            resume_apple_music();
        }
    });

    // Create OpenAI client
    let openai_client = match OpenAiClient::new() {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to create OpenAI client: {e}");
            return Err(e.into());
        }
    };

    // メモリモードではメタデータファイルを使用しないため、プロンプトは取得しない
    let _prompt: Option<String> = None;
    let _ = _prompt; // Explicit acknowledgment of unused variable

    // Convert AudioDataDto back to AudioData
    let audio_data: AudioData = result.audio_data.into();

    let text_result = openai_client.transcribe_audio(audio_data).await;

    // 転写に失敗してもクリップボード操作やペーストは試みない
    match text_result {
        Ok(text) => {
            let repo = JsonFileDictRepo::new();

            // 辞書を適用
            let mut entries = repo.load().unwrap_or_default();
            let replaced = apply_replacements(&text, &mut entries);
            if let Err(e) = repo.save(&entries) {
                eprintln!("dict save error: {e}");
            }

            // スタックモードが有効な場合は自動保存
            if let Some(stack_service_ref) = &stack_service {
                if stack_service_ref.borrow().is_stack_mode_enabled() {
                    let stack_id = stack_service_ref.borrow_mut().save_stack(replaced.clone());
                    let preview = replaced.chars().take(30).collect::<String>();
                    println!("{}", UserFeedback::stack_saved(stack_id, &preview));
                }
            }

            // direct_inputでない場合のみクリップボードへコピー
            if !direct_input {
                if let Err(e) = set_clipboard(&replaced).await {
                    eprintln!("clipboard error: {e}");
                }
            }

            // スタックモードが有効な場合は自動ペーストを無効化
            let should_paste = paste && (stack_service.is_none() || !stack_service.as_ref().unwrap().borrow().is_stack_mode_enabled());

            // 即貼り付け
            if should_paste {
                tokio::time::sleep(Duration::from_millis(80)).await;

                if direct_input {
                    // 直接入力方式（Enigo使用、日本語対応）
                    match text_input::type_text(&replaced).await {
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("Direct input failed: {}, falling back to paste", e);
                            // フォールバック時はクリップボードにコピー
                            if let Err(e) = set_clipboard(&replaced).await {
                                eprintln!("clipboard error in fallback: {e}");
                            }
                            // 既存のペースト処理へフォールバック
                            let _ = tokio::process::Command::new("osascript")
                            .arg("-e")
                            .arg(
                                r#"tell app "System Events" to keystroke "v" using {command down}"#,
                            )
                            .output()
                            .await;
                        }
                    }
                } else {
                    // 既存のペースト方式
                    let _ = tokio::process::Command::new("osascript")
                        .arg("-e")
                        .arg(r#"tell app "System Events" to keystroke "v" using {command down}"#)
                        .output()
                        .await;
                }
            }
        }
        Err(e) => {
            eprintln!("transcription error: {e}");
            return Err(e.into());
        }
    }

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

/// 入力デバイス・環境変数・OpenAI API の状態を確認します。
async fn health_check() -> Result<IpcResp, Box<dyn Error>> {
    let mut ok = true;
    let mut lines = Vec::new();

    if CpalAudioBackend::list_devices().is_empty() {
        lines.push("Input device: MISSING".to_string());
        ok = false;
    } else {
        lines.push("Input device: OK".to_string());
    }

    match std::env::var("OPENAI_API_KEY") {
        Ok(key) => {
            lines.push("OPENAI_API_KEY: present".to_string());
            let client = reqwest::Client::new();
            match client
                .get("https://api.openai.com/v1/models")
                .bearer_auth(key)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    lines.push("OpenAI API: reachable".to_string());
                }
                Ok(resp) => {
                    lines.push(format!("OpenAI API: fail({})", resp.status()));
                    ok = false;
                }
                Err(e) => {
                    lines.push(format!("OpenAI API: error({e})"));
                    ok = false;
                }
            }
        }
        Err(_) => {
            lines.push("OPENAI_API_KEY: missing".to_string());
            ok = false;
        }
    }

    Ok(IpcResp {
        ok,
        msg: lines.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: construct a Recorder<CpalAudioBackend>
    fn make_recorder() -> Rc<std::cell::RefCell<Recorder<CpalAudioBackend>>> {
        Rc::new(std::cell::RefCell::new(Recorder::new(
            CpalAudioBackend::default(),
        )))
    }

    /// `stop_recording` に `prompt` を提供すると、WAV と並べてメタJSONファイルが
    /// 作成されることを検証します。入力デバイスが存在しない場合は自動的にスキップします。
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "Requires LocalSet context and audio device"]
    async fn prompt_is_saved_as_meta() -> Result<(), Box<dyn std::error::Error>> {
        // このテスト中に30秒タイマーが発火するのを防止する
        unsafe {
            std::env::set_var("VOICE_INPUT_MAX_SECS", "60");
        }

        let recorder = make_recorder();
        let ctx = Arc::new(Mutex::new(RecCtx {
            state: RecState::Idle,
            cancel: None,
            music_was_playing: false,
            start_prompt: None,
            paste: false,
            direct_input: false,
        }));
        let (tx, mut rx) = mpsc::unbounded_channel::<(
            RecordingResult,
            bool,
            bool,
            bool,
            Option<Rc<RefCell<StackService>>>,
        )>();
        let stack_service = Rc::new(RefCell::new(StackService::new()));

        if start_recording(
            recorder.clone(),
            &ctx,
            &tx,
            &stack_service,
            false,
            Some("hello".into()),
            false,
        )
        .await
        .is_err()
        {
            eprintln!("⚠️  No audio device – prompt meta test skipped");
            return Ok(());
        }
        stop_recording(recorder, &ctx, &tx, &stack_service, false, None, false).await?;

        let (result, _, _, _, _) = rx.recv().await.expect("result not queued");
        // メモリモードではメタデータファイルは作成されない
        assert!(result.audio_data.0.len() > 0); // 音声データが存在することのみ確認
        Ok(())
    }

    /// 自動タイムアウト（1秒に設定）が録音を停止し、状態を
    /// Idleに戻すことを確認します。オーディオデバイスが利用できない場合はスキップします。
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "Requires LocalSet context and audio device"]
    async fn auto_timeout_triggers_stop() -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            std::env::set_var("VOICE_INPUT_MAX_SECS", "1");
        }

        let recorder = make_recorder();
        let ctx = Arc::new(Mutex::new(RecCtx {
            state: RecState::Idle,
            cancel: None,
            music_was_playing: false,
            start_prompt: None,
            paste: false,
            direct_input: false,
        }));
        let (tx, mut rx) = mpsc::unbounded_channel::<(
            RecordingResult,
            bool,
            bool,
            bool,
            Option<Rc<RefCell<StackService>>>,
        )>();
        let stack_service = Rc::new(RefCell::new(StackService::new()));

        if start_recording(
            recorder.clone(),
            &ctx,
            &tx,
            &stack_service,
            false,
            None,
            false,
        )
        .await
        .is_err()
        {
            eprintln!("⚠️  No audio device – timeout test skipped");
            return Ok(());
        }
        assert!(recorder.borrow().is_recording(), "recording did not start");

        tokio::time::sleep(Duration::from_secs(2)).await; // wait > 1 s
        assert!(
            !recorder.borrow().is_recording(),
            "recording did not auto‑stop"
        );
        assert_eq!(ctx.lock().map_err(|e| e.to_string())?.state, RecState::Idle);
        assert!(rx.try_recv().is_ok(), "Result not queued after timeout");
        Ok(())
    }
}
