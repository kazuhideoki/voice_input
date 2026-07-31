//! 録音状態HUDヘルパーとの連携。
//!
//! daemon本体は音声処理を優先するため、UIは別プロセスへJSON Linesで状態だけを送る。

use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

use tokio::sync::mpsc as tokio_mpsc;

use crate::application::AudioFrame;
use crate::infrastructure::config::AppConfig;
use crate::utils::config::EnvConfig;

const VOICE_RMS_THRESHOLD: f64 = 900.0;
const SILENT_FRAMES_BEFORE_DETECTING: usize = 14;

static HUD_WORKER: OnceLock<Mutex<HudWorker>> = OnceLock::new();

/// HUDに表示する録音ライフサイクル状態。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudState {
    /// 録音中だが、まだ有音を検知していない。
    Detecting,
    /// 有音を検知して録音している。
    Recording,
    /// キーを離して転写中。
    Transcribing,
    /// HUDを隠す。
    Hidden,
}

impl HudState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Detecting => "detecting",
            Self::Recording => "recording",
            Self::Transcribing => "transcribing",
            Self::Hidden => "hidden",
        }
    }
}

struct HudWorker {
    sender: Option<Sender<String>>,
}

impl HudWorker {
    fn new(config: &EnvConfig) -> Self {
        let helper_path = config
            .ui
            .recording_hud_helper_path
            .clone()
            .unwrap_or_else(default_helper_path);
        let Some(sender) = spawn_helper(&helper_path, config.ui.recording_hud_log_path.as_deref())
        else {
            return Self { sender: None };
        };

        Self {
            sender: Some(sender),
        }
    }

    fn send(&mut self, line: String) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };

        if sender.send(line).is_err() {
            self.sender = None;
        }
    }
}

/// HUDワーカーを初期化する。
pub fn init_worker() {
    let config = EnvConfig::get();
    let _ = HUD_WORKER.get_or_init(|| Mutex::new(HudWorker::new(&config)));
}

/// HUDに状態を送る。
pub fn set_state(state: HudState) {
    set_state_with_level(state, None);
}

/// 録音中のPCMフレームを受け取り、有音/無音に応じて`detecting`と`recording`を切り替える。
pub fn start_voice_activity_monitor() -> tokio_mpsc::UnboundedSender<AudioFrame> {
    let (tx, mut rx) = tokio_mpsc::unbounded_channel::<AudioFrame>();
    tokio::task::spawn_local(async move {
        let mut current = HudState::Detecting;
        let mut silent_frames = 0usize;
        set_state(current);

        while let Some(frame) = rx.recv().await {
            let level = normalized_rms(&frame);
            if level > 0.0 {
                silent_frames = 0;
                if current != HudState::Recording {
                    current = HudState::Recording;
                }
                set_state_with_level(HudState::Recording, Some(level));
                continue;
            }

            silent_frames = silent_frames.saturating_add(1);
            if current == HudState::Recording && silent_frames >= SILENT_FRAMES_BEFORE_DETECTING {
                current = HudState::Detecting;
                set_state(current);
            }
        }
    });
    tx
}

fn set_state_with_level(state: HudState, level: Option<f64>) {
    if !AppConfig::load_runtime().effective_recording_hud_enabled() {
        return;
    }
    init_worker();
    let Some(worker) = HUD_WORKER.get() else {
        return;
    };

    let mut line = format!(r#"{{"state":"{}""#, state.as_str());
    if let Some(level) = level {
        let _ = write!(&mut line, r#","level":{:.3}"#, level.clamp(0.0, 1.0));
    }
    line.push('}');

    match worker.lock() {
        Ok(mut worker) => worker.send(line),
        Err(error) => eprintln!("Recording HUD worker lock failed: {error}"),
    }
}

fn normalized_rms(frame: &AudioFrame) -> f64 {
    if frame.samples.is_empty() {
        return 0.0;
    }

    let sum = frame
        .samples
        .iter()
        .map(|sample| {
            let value = f64::from(*sample);
            value * value
        })
        .sum::<f64>();
    let rms = (sum / frame.samples.len() as f64).sqrt();
    if rms < VOICE_RMS_THRESHOLD {
        0.0
    } else {
        (rms / f64::from(i16::MAX)).clamp(0.05, 1.0)
    }
}

fn default_helper_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("voice_input_hud")))
        .unwrap_or_else(|| PathBuf::from("voice_input_hud"))
}

fn spawn_helper(helper_path: &Path, log_path: Option<&Path>) -> Option<Sender<String>> {
    if !cfg!(target_os = "macos") || !helper_path.is_file() {
        return None;
    }

    let mut command = Command::new(helper_path);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    if let Some(path) = log_path {
        command.env("VOICE_INPUT_RECORDING_HUD_LOG_PATH", path);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!(
                "Recording HUD helper failed to start ({}): {}",
                helper_path.display(),
                error
            );
            return None;
        }
    };

    let stdin = child.stdin.take()?;
    Some(spawn_writer_thread(child, stdin))
}

fn spawn_writer_thread(mut child: Child, mut stdin: ChildStdin) -> Sender<String> {
    let (tx, rx) = mpsc::channel::<String>();
    let _ = thread::Builder::new()
        .name("voice-input-hud-writer".to_string())
        .spawn(move || {
            while let Ok(line) = rx.recv() {
                if writeln!(stdin, "{line}").is_err() {
                    break;
                }
                let _ = stdin.flush();
            }
            let _ = child.kill();
            let _ = child.wait();
        });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 無音フレームの音量は0として扱う
    #[test]
    fn silent_frame_has_zero_level() {
        let frame = AudioFrame {
            samples: vec![0; 128],
            sample_rate: 48_000,
            channels: 1,
        };

        assert_eq!(normalized_rms(&frame), 0.0);
    }

    /// 閾値を超えるフレームは有音レベルに変換される
    #[test]
    fn voiced_frame_has_positive_level() {
        let frame = AudioFrame {
            samples: vec![3_000; 128],
            sample_rate: 48_000,
            channels: 1,
        };

        assert!(normalized_rms(&frame) > 0.0);
    }
}
