//! OpenAI Realtime transcription adapter for `gpt-realtime-whisper`.

use base64::Engine;
use futures::{Sink, SinkExt, Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{
        HeaderValue, Request,
        header::{AUTHORIZATION, HeaderName},
    },
};

use crate::application::{AudioFrame, TranscriptionClientError, TranscriptionEvent};
use crate::domain::transcription::TranscriptionOutput;
use crate::error::{Result, VoiceInputError};
use crate::utils::config::TranscriptionConfig;
use crate::utils::profiling;

const OPENAI_REALTIME_TRANSCRIPTION_URL: &str =
    "wss://api.openai.com/v1/realtime?intent=transcription";
const REALTIME_WHISPER_MODEL: &str = "gpt-realtime-whisper";
const TRANSCRIPTION_LANGUAGE: &str = "ja";
const REALTIME_SAMPLE_RATE: u32 = 24_000;
const APPEND_CHUNK_MS: usize = 100;
const APPEND_CHUNK_BYTES: usize = REALTIME_SAMPLE_RATE as usize * 2 * APPEND_CHUNK_MS / 1000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_UPDATE_TIMEOUT: Duration = Duration::from_secs(10);
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(30);
const FINISH_TIMEOUT: Duration = Duration::from_secs(45);
const READY_TIMEOUT: Duration = Duration::from_secs(25);
const OPENAI_SAFETY_IDENTIFIER: HeaderName = HeaderName::from_static("openai-safety-identifier");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealtimeWhisperConfig {
    api_key: String,
}

impl RealtimeWhisperConfig {
    /// 転写設定から Realtime Whisper 設定を作成する。
    pub fn from_transcription_config(config: &TranscriptionConfig) -> Result<Self> {
        let api_key = config.api_key.clone().ok_or_else(|| {
            VoiceInputError::from(TranscriptionClientError::Initialization {
                message: "OPENAI_API_KEY environment variable is not set".to_string(),
            })
        })?;

        Ok(Self { api_key })
    }
}

/// 実行中の Realtime Whisper セッション。
pub struct RealtimeWhisperSessionHandle {
    frame_tx: mpsc::UnboundedSender<AudioFrame>,
    stop_tx: Option<oneshot::Sender<()>>,
    ready_rx: Option<oneshot::Receiver<Result<()>>>,
    result_rx: oneshot::Receiver<Result<TranscriptionOutput>>,
    task: JoinHandle<()>,
}

impl RealtimeWhisperSessionHandle {
    /// セッションへ録音中 PCM フレームを送るための sender。
    pub fn frame_tx(&self) -> mpsc::UnboundedSender<AudioFrame> {
        self.frame_tx.clone()
    }

    /// WebSocket 接続と `session.update` ACK を待ち、音声送信可能な状態にする。
    pub async fn wait_until_ready(&mut self) -> Result<()> {
        let Some(mut ready_rx) = self.ready_rx.take() else {
            return Ok(());
        };

        match tokio::time::timeout(READY_TIMEOUT, &mut ready_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(request_error(&format!(
                "realtime-whisper ready signal was cancelled: {}",
                error
            ))),
            Err(_) => {
                self.task.abort();
                Err(request_error(
                    "realtime-whisper session was not ready within 25s",
                ))
            }
        }
    }

    /// セッション task がすでに終了しているかどうか。
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    /// 音声送信を commit し、最終 transcript を待つ。
    pub async fn finish(mut self) -> Result<TranscriptionOutput> {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }

        match tokio::time::timeout(FINISH_TIMEOUT, &mut self.result_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(VoiceInputError::from(TranscriptionClientError::Request {
                message: format!("realtime-whisper session ended without result: {}", error),
            })),
            Err(_) => {
                self.task.abort();
                Err(request_error(
                    "realtime-whisper session did not finish within 45s after stop",
                ))
            }
        }
    }

    /// セッション task を中止する。
    pub fn abort(self) {
        self.task.abort();
    }
}

impl Drop for RealtimeWhisperSessionHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Realtime Whisper セッションをバックグラウンドで開始する。
pub fn spawn_realtime_whisper_session(
    config: RealtimeWhisperConfig,
    event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
) -> RealtimeWhisperSessionHandle {
    let (frame_tx, frame_rx) = mpsc::unbounded_channel();
    let (stop_tx, stop_rx) = oneshot::channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let (result_tx, result_rx) = oneshot::channel();

    let task = tokio::task::spawn_local(async move {
        let result =
            run_realtime_whisper_session(config, frame_rx, stop_rx, event_tx, ready_tx).await;
        let _ = result_tx.send(result);
    });

    RealtimeWhisperSessionHandle {
        frame_tx,
        stop_tx: Some(stop_tx),
        ready_rx: Some(ready_rx),
        result_rx,
        task,
    }
}

async fn run_realtime_whisper_session(
    config: RealtimeWhisperConfig,
    mut frame_rx: mpsc::UnboundedReceiver<AudioFrame>,
    mut stop_rx: oneshot::Receiver<()>,
    event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ready_tx: oneshot::Sender<Result<()>>,
) -> Result<TranscriptionOutput> {
    let overall_timer = profiling::Timer::start("realtime_whisper.session");
    let mut ready_tx = Some(ready_tx);
    let request = match build_realtime_request(&config.api_key) {
        Ok(request) => request,
        Err(error) => {
            notify_ready(&mut ready_tx, Err(request_error(&error.to_string())));
            return Err(error);
        }
    };

    profiling::log_point("realtime_whisper.websocket.connecting", "");
    let (stream, _) = match tokio::time::timeout(CONNECT_TIMEOUT, connect_async(request)).await {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            let error = VoiceInputError::from(TranscriptionClientError::Request {
                message: format!("failed to connect realtime websocket: {}", error),
            });
            notify_ready(&mut ready_tx, Err(request_error(&error.to_string())));
            return Err(error);
        }
        Err(_) => {
            let error = request_error("realtime websocket connection did not open within 10s");
            notify_ready(&mut ready_tx, Err(request_error(&error.to_string())));
            return Err(error);
        }
    };
    profiling::log_point("realtime_whisper.websocket.connected", "");
    let (mut write, mut read) = stream.split();

    if let Err(error) = send_json(&mut write, build_session_update_payload()).await {
        notify_ready(&mut ready_tx, Err(request_error(&error.to_string())));
        return Err(error);
    }
    profiling::log_point("realtime_whisper.session_update.sent", "");

    match tokio::time::timeout(SESSION_UPDATE_TIMEOUT, wait_for_session_updated(&mut read)).await {
        Ok(Ok(())) => {
            profiling::log_point("realtime_whisper.session_update.ack", "");
            notify_ready(&mut ready_tx, Ok(()));
        }
        Ok(Err(error)) => {
            notify_ready(&mut ready_tx, Err(request_error(&error.to_string())));
            return Err(error);
        }
        Err(_) => {
            let error = request_error("realtime session.update was not acknowledged within 10s");
            notify_ready(&mut ready_tx, Err(request_error(&error.to_string())));
            return Err(error);
        }
    }

    let mut converter = Pcm24kMonoConverter::default();
    let mut pending_audio = Vec::new();
    let mut committed = false;
    let completion_timeout = tokio::time::sleep(COMPLETION_TIMEOUT);
    tokio::pin!(completion_timeout);
    let mut completion_timeout_enabled = false;

    loop {
        tokio::select! {
            message = read.next() => {
                let Some(message) = message else {
                    return Err(request_error("realtime websocket closed before final transcript"));
                };
                if let Some(output) = handle_realtime_message(message, &event_tx).await? {
                    if profiling::enabled() {
                        overall_timer.log_with(&format!("text_len={}", output.text.len()));
                    } else {
                        overall_timer.log();
                    }
                    return Ok(output);
                }
            }
            frame = frame_rx.recv(), if !committed => {
                if let Some(frame) = frame {
                    append_converted_frame(&mut write, &mut converter, &mut pending_audio, frame).await?;
                }
            }
            stop = &mut stop_rx, if !committed => {
                let _ = stop;
                while let Ok(frame) = frame_rx.try_recv() {
                    append_converted_frame(&mut write, &mut converter, &mut pending_audio, frame).await?;
                }
                flush_pending_audio(&mut write, &mut pending_audio, true).await?;
                send_json(&mut write, json!({"type": "input_audio_buffer.commit"})).await?;
                profiling::log_point("realtime_whisper.audio.commit", "");
                committed = true;
                completion_timeout.as_mut().reset(Instant::now() + COMPLETION_TIMEOUT);
                completion_timeout_enabled = true;
            }
            _ = &mut completion_timeout, if completion_timeout_enabled => {
                return Err(request_error("realtime transcription did not complete within 30s after commit"));
            }
        }
    }
}

fn notify_ready(ready_tx: &mut Option<oneshot::Sender<Result<()>>>, result: Result<()>) {
    if let Some(tx) = ready_tx.take() {
        let _ = tx.send(result);
    }
}

fn build_realtime_request(api_key: &str) -> Result<Request<()>> {
    let mut request = OPENAI_REALTIME_TRANSCRIPTION_URL
        .into_client_request()
        .map_err(|error| {
            VoiceInputError::from(TranscriptionClientError::Initialization {
                message: format!("failed to build realtime websocket request: {}", error),
            })
        })?;

    let authorization = HeaderValue::from_str(&format!("Bearer {}", api_key)).map_err(|error| {
        VoiceInputError::from(TranscriptionClientError::Initialization {
            message: format!("failed to build realtime websocket auth header: {}", error),
        })
    })?;
    request.headers_mut().insert(AUTHORIZATION, authorization);
    request.headers_mut().insert(
        OPENAI_SAFETY_IDENTIFIER,
        HeaderValue::from_static("voice-input"),
    );

    Ok(request)
}

fn build_session_update_payload() -> serde_json::Value {
    json!({
        "type": "session.update",
        "session": {
            "type": "transcription",
            "audio": {
                "input": {
                    "format": {
                        "type": "audio/pcm",
                        "rate": REALTIME_SAMPLE_RATE,
                    },
                    "transcription": {
                        "model": REALTIME_WHISPER_MODEL,
                        "language": TRANSCRIPTION_LANGUAGE,
                    },
                    "turn_detection": null,
                }
            },
        }
    })
}

async fn wait_for_session_updated<R>(read: &mut R) -> Result<()>
where
    R: Stream<Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let Some(message) = read.next().await else {
            return Err(request_error(
                "realtime websocket closed before session.updated",
            ));
        };
        let message = message.map_err(map_ws_error)?;
        if !message.is_text() {
            continue;
        }
        let event = parse_realtime_event(message.to_text().map_err(map_ws_error)?)?;
        match event.event_type.as_str() {
            "session.updated" => return Ok(()),
            "error" => return Err(request_error(&event.error_message())),
            _ => {}
        }
    }
}

async fn handle_realtime_message(
    message: std::result::Result<Message, tokio_tungstenite::tungstenite::Error>,
    event_tx: &mpsc::UnboundedSender<TranscriptionEvent>,
) -> Result<Option<TranscriptionOutput>> {
    let message = message.map_err(map_ws_error)?;
    if !message.is_text() {
        return Ok(None);
    }

    let event = parse_realtime_event(message.to_text().map_err(map_ws_error)?)?;
    match event.event_type.as_str() {
        "conversation.item.input_audio_transcription.delta" => {
            if let Some(delta) = event.text_delta() {
                let _ = event_tx.send(TranscriptionEvent::Delta(delta));
            }
            Ok(None)
        }
        "conversation.item.input_audio_transcription.completed" => {
            Ok(Some(event.into_transcription_output()))
        }
        "error" => Err(request_error(&event.error_message())),
        _ => Ok(None),
    }
}

async fn append_converted_frame<W>(
    write: &mut W,
    converter: &mut Pcm24kMonoConverter,
    pending_audio: &mut Vec<u8>,
    frame: AudioFrame,
) -> Result<()>
where
    W: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    pending_audio.extend(converter.convert_frame(&frame));
    flush_pending_audio(write, pending_audio, false).await
}

async fn flush_pending_audio<W>(
    write: &mut W,
    pending_audio: &mut Vec<u8>,
    flush_all: bool,
) -> Result<()>
where
    W: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    while pending_audio.len() >= APPEND_CHUNK_BYTES {
        let chunk: Vec<u8> = pending_audio.drain(..APPEND_CHUNK_BYTES).collect();
        send_audio_append(write, &chunk).await?;
    }

    if flush_all && !pending_audio.is_empty() {
        let chunk = std::mem::take(pending_audio);
        send_audio_append(write, &chunk).await?;
    }

    Ok(())
}

async fn send_audio_append<W>(write: &mut W, pcm: &[u8]) -> Result<()>
where
    W: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let audio = base64::engine::general_purpose::STANDARD.encode(pcm);
    send_json(
        write,
        json!({
            "type": "input_audio_buffer.append",
            "audio": audio,
        }),
    )
    .await
}

async fn send_json<W>(write: &mut W, payload: serde_json::Value) -> Result<()>
where
    W: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    write
        .send(Message::Text(payload.to_string().into()))
        .await
        .map_err(map_ws_error)
}

fn parse_realtime_event(text: &str) -> Result<RealtimeEvent> {
    serde_json::from_str(text).map_err(|error| {
        VoiceInputError::from(TranscriptionClientError::Request {
            message: format!("failed to parse realtime event: {}", error),
        })
    })
}

fn map_ws_error(error: tokio_tungstenite::tungstenite::Error) -> VoiceInputError {
    VoiceInputError::from(TranscriptionClientError::Request {
        message: error.to_string(),
    })
}

fn request_error(message: &str) -> VoiceInputError {
    VoiceInputError::from(TranscriptionClientError::Request {
        message: message.to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct RealtimeEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<String>,
    text: Option<String>,
    transcript: Option<String>,
    error: Option<RealtimeErrorPayload>,
}

impl RealtimeEvent {
    fn text_delta(&self) -> Option<String> {
        self.delta
            .clone()
            .or_else(|| self.text.clone())
            .or_else(|| self.transcript.clone())
    }

    fn into_transcription_output(self) -> TranscriptionOutput {
        let text = self
            .delta
            .or(self.text)
            .or(self.transcript)
            .unwrap_or_default();
        TranscriptionOutput::from_text(text)
    }

    fn error_message(&self) -> String {
        self.error
            .as_ref()
            .and_then(|error| error.message.clone())
            .unwrap_or_else(|| "realtime API returned an error event".to_string())
    }
}

#[derive(Debug, Deserialize)]
struct RealtimeErrorPayload {
    message: Option<String>,
}

#[derive(Default)]
struct Pcm24kMonoConverter {
    source_rate: Option<u32>,
    carry: Vec<i16>,
    source_position: f64,
}

impl Pcm24kMonoConverter {
    fn convert_frame(&mut self, frame: &AudioFrame) -> Vec<u8> {
        if frame.samples.is_empty() || frame.sample_rate == 0 || frame.channels == 0 {
            return Vec::new();
        }

        if self.source_rate != Some(frame.sample_rate) {
            self.source_rate = Some(frame.sample_rate);
            self.carry.clear();
            self.source_position = 0.0;
        }

        let mono = downmix_to_mono(&frame.samples, frame.channels);
        let resampled = self.resample_mono(&mono, frame.sample_rate);
        pcm_i16_le_bytes(&resampled)
    }

    fn resample_mono(&mut self, mono: &[i16], source_rate: u32) -> Vec<i16> {
        if source_rate == REALTIME_SAMPLE_RATE {
            return mono.to_vec();
        }

        self.carry.extend_from_slice(mono);
        let step = source_rate as f64 / REALTIME_SAMPLE_RATE as f64;
        let mut output = Vec::new();

        while self.source_position + 1.0 < self.carry.len() as f64 {
            let left_index = self.source_position.floor() as usize;
            let right_index = left_index + 1;
            let fraction = self.source_position - left_index as f64;
            let left = self.carry[left_index] as f64;
            let right = self.carry[right_index] as f64;
            let value = left * (1.0 - fraction) + right * fraction;
            output.push(value.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16);
            self.source_position += step;
        }

        let consumed = self.source_position.floor() as usize;
        if consumed > 0 {
            self.carry.drain(..consumed);
            self.source_position -= consumed as f64;
        }

        output
    }
}

fn downmix_to_mono(samples: &[i16], channels: u16) -> Vec<i16> {
    let channels = channels.max(1) as usize;
    if channels == 1 {
        return samples.to_vec();
    }

    samples
        .chunks(channels)
        .map(|frame| {
            let sum: i32 = frame.iter().map(|sample| *sample as i32).sum();
            (sum / frame.len() as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16
        })
        .collect()
}

fn pcm_i16_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Realtime API の delta event から増分文字列を取り出せる
    #[test]
    fn realtime_delta_event_parses_text() {
        let event = parse_realtime_event(
            r#"{"type":"conversation.item.input_audio_transcription.delta","delta":"こん"}"#,
        )
        .unwrap();

        assert_eq!(event.text_delta().as_deref(), Some("こん"));
    }

    /// Realtime API の session.update はモデルと言語を固定する
    #[test]
    fn session_update_uses_fixed_model_and_language() {
        let payload = build_session_update_payload();

        assert_eq!(
            payload["session"]["audio"]["input"]["transcription"]["model"],
            json!("gpt-realtime-whisper")
        );
        assert_eq!(
            payload["session"]["audio"]["input"]["transcription"]["language"],
            json!("ja")
        );
        assert!(payload["session"].get("include").is_none());
    }

    /// Realtime API の完了 event から最終文字列を取り出せる
    #[test]
    fn realtime_completed_event_returns_transcript() {
        let event = parse_realtime_event(
            r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"こんにちは"}"#,
        )
        .unwrap();

        assert_eq!(
            event.into_transcription_output(),
            TranscriptionOutput::from_text("こんにちは")
        );
    }

    /// ステレオ入力は平均化されてモノラルPCMへ変換される
    #[test]
    fn stereo_audio_is_downmixed_to_mono_pcm() {
        let bytes = Pcm24kMonoConverter::default().convert_frame(&AudioFrame {
            samples: vec![1000, 3000, -1000, 1000],
            sample_rate: REALTIME_SAMPLE_RATE,
            channels: 2,
        });

        assert_eq!(bytes, vec![0xd0, 0x07, 0x00, 0x00]);
    }

    /// 48kHz入力はRealtime API用の24kHzへ間引かれる
    #[test]
    fn forty_eight_khz_audio_is_resampled_to_twenty_four_khz() {
        let samples = vec![1000i16; 48_000];
        let bytes = Pcm24kMonoConverter::default().convert_frame(&AudioFrame {
            samples,
            sample_rate: 48_000,
            channels: 1,
        });

        assert_eq!(bytes.len(), 24_000 * 2);
    }

    /// Realtime WebSocket 接続には tungstenite が生成する必須ハンドシェイクヘッダーを含める
    #[test]
    fn realtime_request_includes_websocket_handshake_headers() {
        let request = build_realtime_request("secret").unwrap();

        assert!(request.headers().contains_key("sec-websocket-key"));
        assert!(request.headers().contains_key("sec-websocket-version"));
        assert_eq!(
            request.headers().get(AUTHORIZATION).unwrap(),
            "Bearer secret"
        );
    }
}
