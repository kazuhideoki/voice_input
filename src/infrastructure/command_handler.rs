//! IPCコマンドハンドラー
//!
//! # 責任
//! - IPCコマンドの処理と適切なサービスへの委譲
//! - サービス間の調整
//! - レスポンスの生成

#![allow(clippy::await_holding_refcell_ref)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tokio::sync::mpsc;
use tokio::task::spawn_local;
use tokio::time::Duration;

use crate::application::{
    AudioData, AudioDataFormat, CapturedAudio, RecordedAudio, RecordingOptions, RecordingService,
    TranscriptionEvent, TranscriptionService,
};
use crate::domain::transcription::FinalizedTranscription;
use crate::error::{Result, VoiceInputError};
use crate::infrastructure::{
    audio::{AudioBackend, CpalAudioBackend},
    external::{
        realtime_whisper_adapter::{
            RealtimeWhisperConfig, RealtimeWhisperSessionHandle, spawn_realtime_whisper_session,
        },
        sound::{play_start_sound, play_stop_sound, resume_apple_music},
    },
    media_control_service::MediaControlService,
    transcription_worker::{maybe_select_low_confidence, process_streaming_text_input},
};
use crate::ipc::{IpcCmd, IpcResp};
use crate::utils::config::{EnvConfig, TranscriptionProvider};
use crate::utils::profiling;

/// 転写メッセージ
#[derive(Clone, Debug)]
pub struct TranscriptionMessage {
    pub result: RecordedAudio,
    pub resume_music: bool,
    pub session_id: u64,
    pub provider: TranscriptionProvider,
}

struct ActiveRealtimeSession {
    session_id: u64,
    session: RealtimeWhisperSessionHandle,
    event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    input_task: tokio::task::JoinHandle<Option<(FinalizedTranscription, bool)>>,
}

struct RealtimeStopOutcome {
    save_result: Option<(PathBuf, Result<PathBuf>)>,
}

/// コマンドハンドラー
pub struct CommandHandler<T: AudioBackend> {
    recording: Rc<RefCell<RecordingService<T>>>,
    #[allow(dead_code)]
    transcription: Rc<RefCell<TranscriptionService>>,
    media_control: Rc<RefCell<MediaControlService>>,
    transcription_tx: mpsc::UnboundedSender<TranscriptionMessage>,
    realtime_session: Rc<RefCell<Option<ActiveRealtimeSession>>>,
}

impl<T: AudioBackend + 'static> CommandHandler<T> {
    /// 新しいCommandHandlerを作成
    pub fn new(
        recording: Rc<RefCell<RecordingService<T>>>,
        transcription: Rc<RefCell<TranscriptionService>>,
        media_control: Rc<RefCell<MediaControlService>>,
        transcription_tx: mpsc::UnboundedSender<TranscriptionMessage>,
    ) -> Self {
        Self {
            recording,
            transcription,
            media_control,
            transcription_tx,
            realtime_session: Rc::new(RefCell::new(None)),
        }
    }

    /// IPCコマンドを処理
    pub async fn handle(&self, cmd: IpcCmd) -> Result<IpcResp> {
        match cmd {
            IpcCmd::Start {
                prompt,
                save_audio_path,
                transcription_provider,
            } => {
                self.handle_start(prompt, save_audio_path, transcription_provider)
                    .await
            }
            IpcCmd::Stop => self.handle_stop().await,
            IpcCmd::Toggle {
                prompt,
                save_audio_path,
                transcription_provider,
            } => {
                if self.recording.borrow().is_recording() {
                    self.handle_stop().await
                } else {
                    self.handle_start(prompt, save_audio_path, transcription_provider)
                        .await
                }
            }
            IpcCmd::Status => self.handle_status(),
            IpcCmd::ListDevices => self.handle_list_devices(),
            IpcCmd::Health => self.handle_health().await,
        }
    }

    /// 録音開始処理
    async fn handle_start(
        &self,
        prompt: Option<String>,
        save_audio_path: Option<std::path::PathBuf>,
        transcription_provider: Option<TranscriptionProvider>,
    ) -> Result<IpcResp> {
        // 体感開始時間を縮めるため、開始音は録音開始前に鳴らす
        play_start_sound();

        let provider = transcription_provider.unwrap_or(TranscriptionProvider::DEFAULT);
        let mut realtime_session = if provider == TranscriptionProvider::OpenAiRealtimeWhisper {
            Some(self.prepare_realtime_session()?)
        } else {
            None
        };
        let audio_frame_tx = realtime_session
            .as_ref()
            .map(|active| active.session.frame_tx());

        // 録音オプションを構築
        let options = RecordingOptions {
            prompt,
            save_audio_path,
            transcription_provider: provider,
            requested_audio_format: requested_audio_format(provider),
            audio_frame_tx,
        };

        // 録音を開始
        let recording = self.recording.clone();
        let session_id = match recording.borrow().start_recording(options).await {
            Ok(session_id) => session_id,
            Err(error) => {
                if let Some(active) = realtime_session.take() {
                    active.session.abort();
                    active.input_task.abort();
                }
                return Err(error);
            }
        };

        if let Some(mut active) = realtime_session {
            active.session_id = session_id;
            *self.realtime_session.borrow_mut() = Some(active);
        }

        // Apple Music の pause は録音開始後に非同期で行う
        self.spawn_pause_if_needed(session_id);

        // 自動停止タイマーを設定
        self.setup_auto_stop_timer();

        let max_secs = self.recording.borrow().config().max_duration_secs;
        Ok(IpcResp {
            ok: true,
            msg: format!("recording started (auto-stop in {}s)", max_secs),
        })
    }

    fn prepare_realtime_session(&self) -> Result<ActiveRealtimeSession> {
        if self.realtime_session.borrow().is_some() {
            return Err(VoiceInputError::SystemError(
                "realtime-whisper session is already active".to_string(),
            ));
        }

        let config = RealtimeWhisperConfig::from_transcription_config(
            &EnvConfig::get().transcription,
            "ja".to_string(),
        )?;
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let input_task =
            spawn_local(async move { process_streaming_text_input(&mut event_rx).await });
        let session = spawn_realtime_whisper_session(config, event_tx.clone());

        Ok(ActiveRealtimeSession {
            session_id: 0,
            session,
            event_tx,
            input_task,
        })
    }

    fn spawn_pause_if_needed(&self, session_id: u64) {
        let media_control = self.media_control.clone();
        let recording = self.recording.clone();

        spawn_local(async move {
            let was_playing = match media_control
                .borrow()
                .pause_if_playing_for_session(session_id)
                .await
            {
                Ok(value) => value,
                Err(err) => {
                    eprintln!(
                        "Apple Music control failed after recording start (session {}): {}",
                        session_id, err
                    );
                    return;
                }
            };

            if !was_playing {
                return;
            }

            if matches!(recording.borrow().is_active_session(session_id), Ok(true)) {
                if let Err(err) = recording.borrow().set_music_was_playing(true) {
                    eprintln!(
                        "Failed to persist music playback state for session {}: {}",
                        session_id, err
                    );
                    let _ = media_control
                        .borrow()
                        .resume_if_paused_for_session(session_id)
                        .await;
                }
                return;
            }

            let _ = media_control
                .borrow()
                .resume_if_paused_for_session(session_id)
                .await;
        });
    }

    /// 録音停止処理
    async fn handle_stop(&self) -> Result<IpcResp> {
        // 停止音を再生
        play_stop_sound();

        if self.realtime_session.borrow().is_some() {
            return self.handle_stop_realtime().await;
        }

        // 録音を停止
        let recording = self.recording.clone();
        let outcome = recording.borrow().stop_recording().await?;
        let audio_bytes = outcome.result.audio_data.bytes.len();
        let save_result = outcome.context.save_audio_path.as_ref().map(|path| {
            (
                path.clone(),
                save_audio_file(path, &outcome.result.audio_data),
            )
        });

        // 転写キューに送信
        self.transcription_tx
            .send(TranscriptionMessage {
                result: outcome.result,
                resume_music: outcome.context.music_was_playing,
                session_id: outcome.context.session_id,
                provider: outcome.context.transcription_provider,
            })
            .map_err(|e| {
                VoiceInputError::SystemError(format!(
                    "Failed to send to transcription queue: {}",
                    e
                ))
            })?;

        if profiling::enabled() {
            profiling::log_point("transcription.queued", &format!("bytes={}", audio_bytes));
        }

        let msg = match save_result {
            Some((_requested_path, Ok(saved_path))) => {
                format!(
                    "recording stopped; queued; audio saved to {}",
                    saved_path.display()
                )
            }
            Some((path, Err(error))) => format!(
                "recording stopped; queued; audio save failed for {}: {}",
                path.display(),
                error
            ),
            None => "recording stopped; queued".to_string(),
        };

        Ok(IpcResp { ok: true, msg })
    }

    async fn handle_stop_realtime(&self) -> Result<IpcResp> {
        let active = self.realtime_session.borrow_mut().take().ok_or_else(|| {
            VoiceInputError::SystemError("realtime session not found".to_string())
        })?;

        let outcome = stop_active_realtime_session(
            self.recording.clone(),
            self.transcription.clone(),
            active,
        )
        .await?;

        let msg = match outcome.save_result {
            Some((_requested_path, Ok(saved_path))) => {
                format!(
                    "recording stopped; realtime-whisper completed; audio saved to {}",
                    saved_path.display()
                )
            }
            Some((path, Err(error))) => format!(
                "recording stopped; realtime-whisper completed; audio save failed for {}: {}",
                path.display(),
                error
            ),
            None => "recording stopped; realtime-whisper completed".to_string(),
        };

        Ok(IpcResp { ok: true, msg })
    }

    /// ステータス取得
    fn handle_status(&self) -> Result<IpcResp> {
        let state = if self.recording.borrow().is_recording() {
            "Recording"
        } else {
            "Idle"
        };

        Ok(IpcResp {
            ok: true,
            msg: format!("state={}", state),
        })
    }

    /// デバイス一覧取得
    fn handle_list_devices(&self) -> Result<IpcResp> {
        let devices = CpalAudioBackend::list_devices();
        Ok(IpcResp {
            ok: true,
            msg: if devices.is_empty() {
                "⚠️  No input devices detected".to_string()
            } else {
                devices.join("\n")
            },
        })
    }

    /// ヘルスチェック
    async fn handle_health(&self) -> Result<IpcResp> {
        let mut ok = true;
        let mut lines = Vec::new();

        // デバイスチェック
        if CpalAudioBackend::list_devices().is_empty() {
            lines.push("Input device: MISSING".to_string());
            ok = false;
        } else {
            lines.push("Input device: OK".to_string());
        }

        let transcription = &EnvConfig::get().transcription;
        match transcription.provider {
            crate::utils::config::TranscriptionProvider::OpenAi4o
            | crate::utils::config::TranscriptionProvider::OpenAiRealtimeWhisper => {
                match transcription.api_key.clone() {
                    Some(key) => {
                        lines.push(format!(
                            "transcription provider: {}",
                            transcription.provider.as_str()
                        ));
                        lines.push("TRANSCRIPTION_API_KEY: present".to_string());
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
                                lines.push(format!("OpenAI API: error({})", e));
                                ok = false;
                            }
                        }
                    }
                    None => {
                        lines.push(format!(
                            "transcription provider: {}",
                            transcription.provider.as_str()
                        ));
                        lines.push("TRANSCRIPTION_API_KEY: missing".to_string());
                        ok = false;
                    }
                }
            }
            crate::utils::config::TranscriptionProvider::MlxQwen3Asr => {
                lines.push("transcription provider: mlx-qwen3-asr".to_string());
                lines.push(format!(
                    "mlx-qwen3-asr command: {}",
                    transcription.mlx_qwen3_asr_command
                ));
                match std::process::Command::new(&transcription.mlx_qwen3_asr_command)
                    .arg("--help")
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        lines.push("mlx-qwen3-asr: reachable".to_string());
                    }
                    Ok(output) => {
                        lines.push(format!(
                            "mlx-qwen3-asr: fail({})",
                            output.status.code().unwrap_or(-1)
                        ));
                        ok = false;
                    }
                    Err(error) => {
                        lines.push(format!("mlx-qwen3-asr: error({})", error));
                        ok = false;
                    }
                }
            }
        }

        Ok(IpcResp {
            ok,
            msg: lines.join("\n"),
        })
    }

    /// 自動停止タイマーをセットアップ
    fn setup_auto_stop_timer(&self) {
        let recording = self.recording.clone();
        let transcription = self.transcription.clone();
        let realtime_session = self.realtime_session.clone();
        let tx = self.transcription_tx.clone();
        let max_secs = recording.borrow().config().max_duration_secs;

        spawn_local(async move {
            // RecordingServiceからキャンセルレシーバーを取得
            let cancel_rx = recording.borrow().take_cancel_receiver();

            if let Some(cancel_rx) = cancel_rx {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(max_secs)) => {
                        // 30秒経過による自動停止
                        if recording.borrow().is_recording() {
                            println!("Auto-stop timer triggered after {}s", max_secs);
                            play_stop_sound();

                            if let Some(active) = realtime_session.borrow_mut().take() {
                                if let Err(error) = stop_active_realtime_session(
                                    recording.clone(),
                                    transcription.clone(),
                                    active,
                                )
                                .await
                                {
                                    eprintln!("Realtime auto-stop failed: {}", error);
                                }
                            } else if let Ok(outcome) = recording.borrow().stop_recording().await {
                                if let Some(path) = &outcome.context.save_audio_path {
                                    if let Err(error) = save_audio_file(
                                        path,
                                        &outcome.result.audio_data,
                                    ) {
                                        eprintln!("Failed to save audio file: {}", error);
                                    }
                                }
                                let _ = tx.send(TranscriptionMessage {
                                    result: outcome.result,
                                    resume_music: outcome.context.music_was_playing,
                                    session_id: outcome.context.session_id,
                                    provider: outcome.context.transcription_provider,
                                });
                            }
                        }
                    }
                    _ = cancel_rx => {
                        // 手動停止によるキャンセル
                        println!("Auto-stop timer cancelled due to manual stop");
                    }
                }
            } else {
                println!("Warning: Could not set up auto-stop timer - no cancel receiver");
            }
        });
    }
}

async fn stop_active_realtime_session<T: AudioBackend + 'static>(
    recording: Rc<RefCell<RecordingService<T>>>,
    transcription: Rc<RefCell<TranscriptionService>>,
    active: ActiveRealtimeSession,
) -> Result<RealtimeStopOutcome> {
    let capture_outcome = match recording.borrow().stop_capture().await {
        Ok(outcome) => outcome,
        Err(error) => {
            active.session.abort();
            active.input_task.abort();
            return Err(error);
        }
    };

    let resume_music_after_transcription = capture_outcome.context.music_was_playing;
    let _defer_guard = scopeguard::guard(resume_music_after_transcription, |should_resume| {
        if should_resume {
            tokio::task::spawn_local(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                resume_apple_music();
            });
        }
    });

    let ActiveRealtimeSession {
        session_id,
        session,
        event_tx,
        input_task,
    } = active;

    if session_id != capture_outcome.context.session_id {
        input_task.abort();
        return Err(VoiceInputError::SystemError(format!(
            "realtime session mismatch: active={} stopped={}",
            session_id, capture_outcome.context.session_id
        )));
    }

    let raw_output = match session.finish().await {
        Ok(output) => output,
        Err(error) => {
            input_task.abort();
            return Err(error);
        }
    };

    let finalized = transcription.borrow().finalize_output(raw_output)?;
    let _ = event_tx.send(TranscriptionEvent::Completed(finalized.clone()));

    match input_task.await {
        Ok(Some((finalized_for_selection, input_succeeded))) if input_succeeded => {
            maybe_select_low_confidence(
                &finalized_for_selection,
                capture_outcome.context.session_id,
                recording.clone(),
            )
            .await;
        }
        Ok(_) => {}
        Err(error) => {
            eprintln!("Realtime streaming input task failed: {}", error);
        }
    }

    let save_result = capture_outcome
        .context
        .save_audio_path
        .as_ref()
        .map(|path| {
            let requested_format =
                audio_format_from_path(path).or(capture_outcome.context.requested_audio_format);
            (
                path.clone(),
                encode_and_save_capture(
                    &recording,
                    capture_outcome.capture.clone(),
                    requested_format,
                    path,
                ),
            )
        });

    Ok(RealtimeStopOutcome { save_result })
}

fn requested_audio_format(provider: TranscriptionProvider) -> Option<AudioDataFormat> {
    match provider {
        TranscriptionProvider::OpenAi4o | TranscriptionProvider::OpenAiRealtimeWhisper => None,
        TranscriptionProvider::MlxQwen3Asr => Some(AudioDataFormat::Wav),
    }
}

fn audio_format_from_path(path: &Path) -> Option<AudioDataFormat> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("wav") => Some(AudioDataFormat::Wav),
        Some(extension) if extension.eq_ignore_ascii_case("flac") => Some(AudioDataFormat::Flac),
        _ => None,
    }
}

fn encode_and_save_capture<T: AudioBackend>(
    recording: &Rc<RefCell<RecordingService<T>>>,
    capture: CapturedAudio,
    requested_format: Option<AudioDataFormat>,
    path: &Path,
) -> Result<PathBuf> {
    let audio = recording
        .borrow()
        .encode_capture(capture, requested_format)?;
    save_audio_file(path, &audio)
}

fn save_audio_file(path: &Path, audio: &AudioData) -> Result<PathBuf> {
    let save_path = audio_save_path(path, audio)?;
    if let Some(parent) = save_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| {
            VoiceInputError::SystemError(format!(
                "Failed to create audio save directory {}: {}",
                parent.display(),
                error
            ))
        })?;
    }

    std::fs::write(&save_path, &audio.bytes).map_err(|error| {
        VoiceInputError::SystemError(format!(
            "Failed to save audio file {}: {}",
            save_path.display(),
            error
        ))
    })?;

    Ok(save_path)
}

fn audio_save_path(path: &Path, audio: &AudioData) -> Result<PathBuf> {
    let extension = audio_extension(audio)?;
    match path.extension().and_then(|value| value.to_str()) {
        Some(path_extension) if path_extension.eq_ignore_ascii_case(extension) => {
            Ok(path.to_path_buf())
        }
        _ => Ok(path.with_extension(extension)),
    }
}

fn audio_extension(audio: &AudioData) -> Result<&'static str> {
    match audio.mime_type {
        "audio/flac" => Ok("flac"),
        "audio/wav" | "audio/x-wav" => Ok("wav"),
        mime_type => Err(VoiceInputError::SystemError(format!(
            "Unsupported audio MIME type for save: {}",
            mime_type
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::RecordingConfig;
    use crate::application::TranscriptionClient;
    use crate::application::{AudioData, DictRepository, Recorder, TranscriptionClientOptions};
    use crate::domain::dict::WordEntry;
    use crate::domain::transcription::TranscriptionOutput;
    use crate::infrastructure::external::sound::{clear_test_sound_runner, set_test_sound_runner};
    use crate::infrastructure::media_control_service::MediaController;
    use async_trait::async_trait;
    use scopeguard::guard;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;
    use tempfile::TempDir;

    static SOUND_TEST_LOCK: StdMutex<()> = StdMutex::new(());

    struct NoopDictRepository;

    impl DictRepository for NoopDictRepository {
        fn load(&self) -> std::io::Result<Vec<WordEntry>> {
            Ok(vec![])
        }

        fn save(&self, _all: &[WordEntry]) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct NoopTranscriptionClient;

    #[async_trait]
    impl TranscriptionClient for NoopTranscriptionClient {
        async fn transcribe(
            &self,
            _audio: AudioData,
            _options: &TranscriptionClientOptions,
        ) -> crate::error::Result<TranscriptionOutput> {
            Ok(TranscriptionOutput::from_text(String::new()))
        }
    }

    struct RecordingOrderBackend {
        started: Arc<AtomicBool>,
        events: Arc<StdMutex<Vec<&'static str>>>,
    }

    impl RecordingOrderBackend {
        fn new(events: Arc<StdMutex<Vec<&'static str>>>) -> Self {
            Self {
                started: Arc::new(AtomicBool::new(false)),
                events,
            }
        }
    }

    impl AudioBackend for RecordingOrderBackend {
        fn start_recording(
            &self,
        ) -> std::result::Result<(), crate::infrastructure::audio::AudioBackendError> {
            self.started.store(true, Ordering::SeqCst);
            self.events.lock().unwrap().push("recording_started");
            Ok(())
        }

        fn stop_recording(
            &self,
        ) -> std::result::Result<AudioData, crate::infrastructure::audio::AudioBackendError>
        {
            self.started.store(false, Ordering::SeqCst);
            Ok(AudioData {
                bytes: vec![0u8; 16],
                mime_type: "audio/wav",
                file_name: "audio.wav".to_string(),
            })
        }

        fn is_recording(&self) -> bool {
            self.started.load(Ordering::SeqCst)
        }
    }

    struct FormatObservedBackend {
        started: Arc<AtomicBool>,
        requested_format: Arc<StdMutex<Option<AudioDataFormat>>>,
    }

    impl FormatObservedBackend {
        fn new() -> Self {
            Self {
                started: Arc::new(AtomicBool::new(false)),
                requested_format: Arc::new(StdMutex::new(None)),
            }
        }
    }

    impl AudioBackend for FormatObservedBackend {
        fn start_recording(
            &self,
        ) -> std::result::Result<(), crate::infrastructure::audio::AudioBackendError> {
            self.started.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn stop_recording(
            &self,
        ) -> std::result::Result<AudioData, crate::infrastructure::audio::AudioBackendError>
        {
            self.started.store(false, Ordering::SeqCst);
            Ok(AudioData {
                bytes: b"fLaC".to_vec(),
                mime_type: "audio/flac",
                file_name: "audio.flac".to_string(),
            })
        }

        fn stop_recording_as(
            &self,
            format: AudioDataFormat,
        ) -> std::result::Result<AudioData, crate::infrastructure::audio::AudioBackendError>
        {
            *self.requested_format.lock().unwrap() = Some(format);
            self.started.store(false, Ordering::SeqCst);
            Ok(AudioData {
                bytes: b"RIFF".to_vec(),
                mime_type: "audio/wav",
                file_name: "audio.wav".to_string(),
            })
        }

        fn is_recording(&self) -> bool {
            self.started.load(Ordering::SeqCst)
        }
    }

    struct DelayedRecordingOrderBackend {
        started: Arc<AtomicBool>,
        events: Arc<StdMutex<Vec<&'static str>>>,
        delay: Duration,
    }

    impl DelayedRecordingOrderBackend {
        fn new(events: Arc<StdMutex<Vec<&'static str>>>, delay: Duration) -> Self {
            Self {
                started: Arc::new(AtomicBool::new(false)),
                events,
                delay,
            }
        }
    }

    impl AudioBackend for DelayedRecordingOrderBackend {
        fn start_recording(
            &self,
        ) -> std::result::Result<(), crate::infrastructure::audio::AudioBackendError> {
            std::thread::sleep(self.delay);
            self.started.store(true, Ordering::SeqCst);
            self.events.lock().unwrap().push("recording_started");
            Ok(())
        }

        fn stop_recording(
            &self,
        ) -> std::result::Result<AudioData, crate::infrastructure::audio::AudioBackendError>
        {
            self.started.store(false, Ordering::SeqCst);
            Ok(AudioData {
                bytes: vec![0u8; 16],
                mime_type: "audio/wav",
                file_name: "audio.wav".to_string(),
            })
        }

        fn is_recording(&self) -> bool {
            self.started.load(Ordering::SeqCst)
        }
    }

    struct TimingObservedBackend<T: AudioBackend> {
        inner: T,
        started_at: Arc<StdMutex<Option<Instant>>>,
    }

    impl<T: AudioBackend> TimingObservedBackend<T> {
        fn new(inner: T, started_at: Arc<StdMutex<Option<Instant>>>) -> Self {
            Self { inner, started_at }
        }
    }

    impl<T: AudioBackend> AudioBackend for TimingObservedBackend<T> {
        fn start_recording(
            &self,
        ) -> std::result::Result<(), crate::infrastructure::audio::AudioBackendError> {
            self.inner.start_recording()?;
            *self.started_at.lock().unwrap() = Some(Instant::now());
            Ok(())
        }

        fn stop_recording(
            &self,
        ) -> std::result::Result<AudioData, crate::infrastructure::audio::AudioBackendError>
        {
            self.inner.stop_recording()
        }

        fn is_recording(&self) -> bool {
            self.inner.is_recording()
        }
    }

    struct DelayedMediaController {
        playing: Arc<AtomicBool>,
        pause_delay: Duration,
    }

    impl DelayedMediaController {
        fn new(initial_playing: bool, pause_delay: Duration) -> Self {
            Self {
                playing: Arc::new(AtomicBool::new(initial_playing)),
                pause_delay,
            }
        }
    }

    #[async_trait]
    impl MediaController for DelayedMediaController {
        async fn is_playing(&self) -> Result<bool> {
            Ok(self.playing.load(Ordering::SeqCst))
        }

        async fn pause(&self) -> Result<()> {
            tokio::time::sleep(self.pause_delay).await;
            self.playing.store(false, Ordering::SeqCst);
            Ok(())
        }

        async fn resume(&self) -> Result<()> {
            self.playing.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct SequencedMediaController {
        playing: Arc<AtomicBool>,
        pause_delays: Arc<StdMutex<VecDeque<Duration>>>,
        is_playing_results: Arc<StdMutex<VecDeque<bool>>>,
    }

    impl SequencedMediaController {
        fn new(
            initial_playing: bool,
            pause_delays: Vec<Duration>,
            is_playing_results: Vec<bool>,
        ) -> Self {
            Self {
                playing: Arc::new(AtomicBool::new(initial_playing)),
                pause_delays: Arc::new(StdMutex::new(pause_delays.into())),
                is_playing_results: Arc::new(StdMutex::new(is_playing_results.into())),
            }
        }
    }

    #[async_trait]
    impl MediaController for SequencedMediaController {
        async fn is_playing(&self) -> Result<bool> {
            Ok(self
                .is_playing_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| self.playing.load(Ordering::SeqCst)))
        }

        async fn pause(&self) -> Result<()> {
            let delay = self
                .pause_delays
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_default();
            tokio::time::sleep(delay).await;
            self.playing.store(false, Ordering::SeqCst);
            Ok(())
        }

        async fn resume(&self) -> Result<()> {
            self.playing.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FailingPauseMediaController;

    #[async_trait]
    impl MediaController for FailingPauseMediaController {
        async fn is_playing(&self) -> Result<bool> {
            Ok(true)
        }

        async fn pause(&self) -> Result<()> {
            Err(VoiceInputError::SystemError("pause failed".to_string()))
        }

        async fn resume(&self) -> Result<()> {
            Ok(())
        }
    }

    fn build_handler<T: AudioBackend + 'static>(
        backend: T,
        media_control: MediaControlService,
    ) -> (
        CommandHandler<T>,
        Rc<RefCell<RecordingService<T>>>,
        Rc<RefCell<MediaControlService>>,
        mpsc::UnboundedReceiver<TranscriptionMessage>,
    ) {
        let recorder = Rc::new(RefCell::new(Recorder::new(backend)));
        let recording = Rc::new(RefCell::new(RecordingService::new(
            recorder,
            RecordingConfig {
                max_duration_secs: 30,
            },
        )));
        let transcription = Rc::new(RefCell::new(TranscriptionService::new(
            Box::new(NoopTranscriptionClient),
            Box::new(NoopDictRepository),
            1,
        )));
        let media_control = Rc::new(RefCell::new(media_control));
        let (tx, rx) = mpsc::unbounded_channel();

        (
            CommandHandler::new(recording.clone(), transcription, media_control.clone(), tx),
            recording,
            media_control,
            rx,
        )
    }

    fn start_cmd() -> IpcCmd {
        IpcCmd::Start {
            prompt: None,
            save_audio_path: None,
            transcription_provider: None,
        }
    }

    /// 停止時に転写キューへsession_id付きで送信される
    #[tokio::test(flavor = "current_thread")]
    async fn stop_enqueues_transcription_message_with_session_id() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend = RecordingOrderBackend::new(Arc::new(StdMutex::new(Vec::new())));
                let media_control = MediaControlService::with_controller(Box::new(
                    DelayedMediaController::new(false, Duration::from_millis(0)),
                ));
                let (handler, _recording, _media_control, mut rx) =
                    build_handler(backend, media_control);

                handler.handle(start_cmd()).await.unwrap();
                handler.handle(IpcCmd::Stop).await.unwrap();

                let message = rx.recv().await.expect("transcription should be queued");
                assert_eq!(message.session_id, 1);
                assert!(!message.resume_music);
            })
            .await;
    }

    /// mlx-qwen3-asr指定の録音はWAV音声とproviderを転写キューへ渡す
    #[tokio::test(flavor = "current_thread")]
    async fn mlx_provider_start_requests_wav_and_queues_provider() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend = FormatObservedBackend::new();
                let requested_format = backend.requested_format.clone();
                let media_control = MediaControlService::with_controller(Box::new(
                    DelayedMediaController::new(false, Duration::from_millis(0)),
                ));
                let (handler, _recording, _media_control, mut rx) =
                    build_handler(backend, media_control);

                handler
                    .handle(IpcCmd::Start {
                        prompt: None,
                        save_audio_path: None,
                        transcription_provider: Some(TranscriptionProvider::MlxQwen3Asr),
                    })
                    .await
                    .unwrap();
                handler.handle(IpcCmd::Stop).await.unwrap();

                let message = rx.recv().await.expect("transcription should be queued");
                assert_eq!(message.provider, TranscriptionProvider::MlxQwen3Asr);
                assert_eq!(
                    *requested_format.lock().unwrap(),
                    Some(AudioDataFormat::Wav)
                );
                assert_eq!(message.result.audio_data.mime_type, "audio/wav");
            })
            .await;
    }

    /// 保存パスが指定された録音は停止時に音声バイト列を書き出す
    #[tokio::test(flavor = "current_thread")]
    async fn stop_persists_audio_bytes_when_save_path_is_configured() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let temp_dir = TempDir::new().unwrap();
                let save_path = temp_dir.path().join("samples/debug.wav");
                let backend = RecordingOrderBackend::new(Arc::new(StdMutex::new(Vec::new())));
                let media_control = MediaControlService::with_controller(Box::new(
                    DelayedMediaController::new(false, Duration::from_millis(0)),
                ));
                let (handler, _recording, _media_control, mut rx) =
                    build_handler(backend, media_control);

                handler
                    .handle(IpcCmd::Start {
                        prompt: None,
                        save_audio_path: Some(save_path.clone()),
                        transcription_provider: None,
                    })
                    .await
                    .unwrap();
                handler.handle(IpcCmd::Stop).await.unwrap();

                assert_eq!(std::fs::read(&save_path).unwrap(), vec![0u8; 16]);
                assert!(rx.recv().await.is_some());
            })
            .await;
    }

    /// 保存パスの拡張子が音声形式と異なる場合は実際の形式に合わせて保存される
    #[test]
    fn save_path_extension_matches_audio_format() {
        let temp_dir = TempDir::new().unwrap();
        let requested_path = temp_dir.path().join("samples/debug.wav");
        let expected_path = temp_dir.path().join("samples/debug.flac");
        let audio = AudioData {
            bytes: b"fLaCencoded".to_vec(),
            mime_type: "audio/flac",
            file_name: "audio.flac".to_string(),
        };

        let saved_path = save_audio_file(&requested_path, &audio).unwrap();

        assert_eq!(saved_path, expected_path);
        assert!(!requested_path.exists());
        assert_eq!(std::fs::read(expected_path).unwrap(), b"fLaCencoded");
    }

    /// 遅いApple Music確認があっても録音開始レスポンスは待たない
    #[tokio::test(flavor = "current_thread")]
    async fn start_returns_without_waiting_for_music_pause() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend = RecordingOrderBackend::new(Arc::new(StdMutex::new(Vec::new())));
                let media_control = MediaControlService::with_controller(Box::new(
                    DelayedMediaController::new(true, Duration::from_millis(200)),
                ));
                let (handler, _recording, _media_control, _rx) =
                    build_handler(backend, media_control);

                let response =
                    tokio::time::timeout(Duration::from_millis(50), handler.handle(start_cmd()))
                        .await;

                assert!(response.is_ok(), "start should not wait for pause");
            })
            .await;
    }

    /// 開始音が録音開始より先に鳴る
    #[tokio::test(flavor = "current_thread")]
    async fn start_sound_plays_before_recording_begins_immediately() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        clear_test_sound_runner();
        let _cleanup = guard((), |_| clear_test_sound_runner());

        let events = Arc::new(StdMutex::new(Vec::new()));
        let sound_events = events.clone();
        set_test_sound_runner(move |path| {
            if path == "/System/Library/Sounds/Ping.aiff" {
                sound_events.lock().unwrap().push("start_sound");
            }
        });

        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend = RecordingOrderBackend::new(events.clone());
                let media_control = MediaControlService::with_controller(Box::new(
                    DelayedMediaController::new(false, Duration::from_millis(0)),
                ));
                let (handler, _recording, _media_control, _rx) =
                    build_handler(backend, media_control);

                handler.handle(start_cmd()).await.unwrap();
            })
            .await;

        assert_eq!(
            *events.lock().unwrap(),
            vec!["start_sound", "recording_started"]
        );
    }

    /// 開始音が録音開始処理より先に鳴る
    #[tokio::test(flavor = "current_thread")]
    async fn start_sound_plays_before_recording_begins() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        clear_test_sound_runner();
        let _cleanup = guard((), |_| clear_test_sound_runner());

        let events = Arc::new(StdMutex::new(Vec::new()));
        let sound_events = events.clone();
        set_test_sound_runner(move |path| {
            if path == "/System/Library/Sounds/Ping.aiff" {
                sound_events.lock().unwrap().push("start_sound");
            }
        });

        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend =
                    DelayedRecordingOrderBackend::new(events.clone(), Duration::from_millis(20));
                let media_control = MediaControlService::with_controller(Box::new(
                    DelayedMediaController::new(false, Duration::from_millis(0)),
                ));
                let (handler, _recording, _media_control, _rx) =
                    build_handler(backend, media_control);

                handler.handle(start_cmd()).await.unwrap();
            })
            .await;

        assert_eq!(
            *events.lock().unwrap(),
            vec!["start_sound", "recording_started"]
        );
    }

    /// 開始音通知の体感待ち時間を観測できる
    #[tokio::test(flavor = "current_thread")]
    async fn start_sound_timing_observation_with_delayed_backend() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        clear_test_sound_runner();
        let _cleanup = guard((), |_| clear_test_sound_runner());

        let recording_started_at = Arc::new(StdMutex::new(None::<Instant>));
        let sound_played_at = Arc::new(StdMutex::new(None::<Instant>));
        let sound_played_at_ref = sound_played_at.clone();
        let request_started_at = Instant::now();

        set_test_sound_runner(move |path| {
            if path == "/System/Library/Sounds/Ping.aiff" {
                *sound_played_at_ref.lock().unwrap() = Some(Instant::now());
            }
        });

        let started_at_ref = recording_started_at.clone();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend = DelayedRecordingOrderBackend::new(
                    Arc::new(StdMutex::new(Vec::new())),
                    Duration::from_millis(60),
                );
                let backend = TimingObservedBackend::new(backend, started_at_ref);
                let media_control = MediaControlService::with_controller(Box::new(
                    DelayedMediaController::new(false, Duration::from_millis(0)),
                ));
                let (handler, _recording, _media_control, _rx) =
                    build_handler(backend, media_control);

                handler.handle(start_cmd()).await.unwrap();
            })
            .await;

        let started_at = recording_started_at.lock().unwrap().unwrap();
        let sound_at = sound_played_at.lock().unwrap().unwrap();
        let sound_from_request_ms = sound_at.duration_since(request_started_at).as_millis();
        let recording_from_request_ms = started_at.duration_since(request_started_at).as_millis();
        println!(
            "start_sound_timing_observation_with_delayed_backend: sound_from_request={} ms, recording_from_request={} ms",
            sound_from_request_ms, recording_from_request_ms
        );
    }

    /// 停止後にpauseが遅れて完了しても再開状態へ戻る
    #[tokio::test(flavor = "current_thread")]
    async fn delayed_pause_after_stop_does_not_leave_music_paused() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend = RecordingOrderBackend::new(Arc::new(StdMutex::new(Vec::new())));
                let controller = DelayedMediaController::new(true, Duration::from_millis(80));
                let playing_ref = controller.playing.clone();
                let media_control = MediaControlService::with_controller(Box::new(controller));
                let (handler, recording, media_control, _rx) =
                    build_handler(backend, media_control);

                handler.handle(start_cmd()).await.unwrap();
                handler.handle(IpcCmd::Stop).await.unwrap();
                tokio::time::sleep(Duration::from_millis(120)).await;

                let (_, music_was_playing) = recording.borrow().get_context_info().unwrap();
                assert!(!music_was_playing);
                assert!(playing_ref.load(Ordering::SeqCst));
                assert!(!media_control.borrow().is_paused_by_recording().unwrap());
            })
            .await;
    }

    /// 前セッションの遅いpause結果は次セッションへ混入しない
    #[tokio::test(flavor = "current_thread")]
    async fn late_pause_from_previous_session_is_ignored_for_next_session() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend = RecordingOrderBackend::new(Arc::new(StdMutex::new(Vec::new())));
                let controller = SequencedMediaController::new(
                    true,
                    vec![Duration::from_millis(80), Duration::from_millis(80)],
                    vec![true, false],
                );
                let playing_ref = controller.playing.clone();
                let media_control = MediaControlService::with_controller(Box::new(controller));
                let (handler, recording, media_control, _rx) =
                    build_handler(backend, media_control);

                handler.handle(start_cmd()).await.unwrap();
                handler.handle(IpcCmd::Stop).await.unwrap();
                handler.handle(start_cmd()).await.unwrap();
                tokio::time::sleep(Duration::from_millis(120)).await;

                let (_, music_was_playing) = recording.borrow().get_context_info().unwrap();
                assert!(!music_was_playing);
                assert!(playing_ref.load(Ordering::SeqCst));
                assert!(!media_control.borrow().is_paused_by_recording().unwrap());
            })
            .await;
    }

    /// 古いpause完了が新しいpause所有権を打ち消さない
    #[tokio::test(flavor = "current_thread")]
    async fn previous_session_pause_does_not_resume_newer_session_music_pause() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend = RecordingOrderBackend::new(Arc::new(StdMutex::new(Vec::new())));
                let controller = SequencedMediaController::new(
                    true,
                    vec![Duration::from_millis(120), Duration::from_millis(10)],
                    vec![true, true],
                );
                let playing_ref = controller.playing.clone();
                let media_control = MediaControlService::with_controller(Box::new(controller));
                let (handler, recording, media_control, _rx) =
                    build_handler(backend, media_control);

                handler.handle(start_cmd()).await.unwrap();
                handler.handle(IpcCmd::Stop).await.unwrap();
                handler.handle(start_cmd()).await.unwrap();
                tokio::time::sleep(Duration::from_millis(160)).await;

                let (_, music_was_playing) = recording.borrow().get_context_info().unwrap();
                assert!(recording.borrow().is_recording());
                assert!(music_was_playing);
                assert!(!playing_ref.load(Ordering::SeqCst));
                assert!(media_control.borrow().is_paused_by_recording().unwrap());
            })
            .await;
    }

    /// Apple Music制御失敗でも録音開始自体は成功し状態が汚れない
    #[tokio::test(flavor = "current_thread")]
    async fn start_succeeds_when_music_control_fails_after_recording_begins() {
        let _sound_guard = SOUND_TEST_LOCK.lock().unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let backend = RecordingOrderBackend::new(Arc::new(StdMutex::new(Vec::new())));
                let media_control =
                    MediaControlService::with_controller(Box::new(FailingPauseMediaController));
                let (handler, recording, media_control, _rx) =
                    build_handler(backend, media_control);

                let response = handler.handle(start_cmd()).await.unwrap();
                tokio::time::sleep(Duration::from_millis(10)).await;

                let (_, music_was_playing) = recording.borrow().get_context_info().unwrap();
                assert!(response.ok);
                assert!(recording.borrow().is_recording());
                assert!(!music_was_playing);
                assert!(!media_control.borrow().is_paused_by_recording().unwrap());
            })
            .await;
    }
}
