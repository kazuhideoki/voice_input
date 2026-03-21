use super::encoder::{self, AudioFormat};
use super::{AudioBackend, AudioBackendError};
use crate::utils::config::EnvConfig;
use crate::utils::profiling;
use audioadapter_buffers::SizeError;
use cpal::{
    Device, DeviceDescription, SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use rubato::{
    Async, FixedAsync, ResampleError, Resampler, ResamplerConstructionError,
    SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::{
    borrow::Cow,
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// 録音データの返却形式（メモリモード専用）
#[derive(Debug, Clone)]
pub struct AudioData {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
    pub file_name: String,
}

/// 録音状態（メモリモード専用）
struct MemoryRecordingState {
    buffer: Arc<Mutex<Vec<i16>>>,
    sample_rate: u32,
    channels: u16,
    generation: u64,
    accepting_input: Arc<AtomicBool>,
}

struct ProcessedAudio<'a> {
    samples: Cow<'a, [i16]>,
    sample_rate: u32,
    channels: u16,
}

struct ResampleOutcome {
    samples: Vec<i16>,
    sample_rate: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StreamIdentity {
    selected_device_key: String,
    sample_format: SampleFormat,
    sample_rate: u32,
    channels: u16,
}

struct ReadyInputStream {
    _stream: Stream,
    identity: StreamIdentity,
}

type CaptureTarget = (Arc<Mutex<Vec<i16>>>, Arc<AtomicBool>, u64);

const TARGET_SAMPLE_RATE: u32 = 16_000;
const MIN_RESAMPLE_FRAMES: usize = 256;
const INPUT_SETUP_REVALIDATION_INTERVAL: Duration = Duration::from_secs(2);

/// Audio processing errors
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("PCM data too large: {0} bytes exceeds u32 max")]
    DataTooLarge(usize),
}

#[derive(Debug, thiserror::Error)]
enum AudioResampleError {
    #[error("resampler construction failed: {0}")]
    Construction(#[from] ResamplerConstructionError),
    #[error("resampling failed: {0}")]
    Processing(#[from] ResampleError),
    #[error("buffer size mismatch: {0}")]
    Buffer(#[from] SizeError),
}

/// CpalAudioBackend 向けのエラー型（public APIの意味が伝わるメッセージ）
#[derive(Debug, thiserror::Error)]
pub enum CpalBackendError {
    #[error("recording is already in progress")]
    AlreadyRecording,
    #[error("no input device available (check INPUT_DEVICE_PRIORITY)")]
    NoInputDevice,
    #[error("unsupported sample format")]
    UnsupportedSampleFormat,
    #[error("not currently recording")]
    NotRecording,
    #[error("recording state not set")]
    RecordingStateNotSet,
}

/// サンプルフォーマット変換トレイト
pub trait Sample {
    fn to_i16(&self) -> i16;
    fn as_pcm_le_bytes(&self) -> [u8; 2];
}

impl Sample for i16 {
    fn to_i16(&self) -> i16 {
        *self
    }
    fn as_pcm_le_bytes(&self) -> [u8; 2] {
        i16::to_le_bytes(*self)
    }
}

impl Sample for f32 {
    fn to_i16(&self) -> i16 {
        (self.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
    }
    fn as_pcm_le_bytes(&self) -> [u8; 2] {
        self.to_i16().to_le_bytes()
    }
}

/// CPAL によるローカルマイク入力実装（メモリモード専用）
pub struct CpalAudioBackend {
    /// ランタイム中の入力ストリーム
    stream: Mutex<Option<ReadyInputStream>>,
    /// 録音フラグ
    recording: Arc<AtomicBool>,
    /// callback が利用中の録音世代
    capture_generation: Arc<AtomicU64>,
    /// stream error 後に次回開始で張り直すべきか
    stream_needs_rebuild: Arc<AtomicBool>,
    /// 録音状態（メモリモード専用）
    recording_state: Arc<Mutex<Option<MemoryRecordingState>>>,
    /// 入力デバイスと設定のキャッシュ
    input_setup_cache: InputSetupCache<CachedInputSetup>,
}

impl Default for CpalAudioBackend {
    fn default() -> Self {
        Self {
            stream: Mutex::new(None),
            recording: Arc::new(AtomicBool::new(false)),
            capture_generation: Arc::new(AtomicU64::new(0)),
            stream_needs_rebuild: Arc::new(AtomicBool::new(false)),
            recording_state: Arc::new(Mutex::new(None)),
            input_setup_cache: InputSetupCache::new(),
        }
    }
}

#[derive(Clone)]
struct CachedInputSetup {
    device: Device,
    supported_config: cpal::SupportedStreamConfig,
    input_device_priority: Vec<String>,
    selected_device_key: String,
    last_validated_at: Arc<Mutex<Instant>>,
    stream_identity: StreamIdentity,
}

struct InputSetupCache<T> {
    value: Mutex<Option<T>>,
}

impl<T> InputSetupCache<T> {
    fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    #[cfg(test)]
    fn clear(&self) {
        *self.value.lock().unwrap() = None;
    }
}

#[cfg(test)]
impl<T: Clone> InputSetupCache<T> {
    fn get_or_try_init<E, F>(&self, init: F) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if let Some(value) = self.value.lock().unwrap().clone() {
            return Ok(value);
        }

        let value = init()?;
        *self.value.lock().unwrap() = Some(value.clone());
        Ok(value)
    }
}

impl<T: Clone> InputSetupCache<T> {
    fn get_or_try_init_if<E, V, F>(&self, is_valid: V, init: F) -> Result<T, E>
    where
        V: FnOnce(&T) -> bool,
        F: FnOnce() -> Result<T, E>,
    {
        if let Some(value) = self.value.lock().unwrap().clone() {
            if is_valid(&value) {
                return Ok(value);
            }
        }

        let value = init()?;
        *self.value.lock().unwrap() = Some(value.clone());
        Ok(value)
    }
}

fn input_device_priorities() -> Vec<String> {
    EnvConfig::get().audio.input_device_priorities.clone()
}

fn select_input_device_with_priorities(
    host: &cpal::Host,
    priorities: &[String],
    should_log: bool,
) -> Option<Device> {
    // 1) 利用可能なデバイスを列挙
    let available: Vec<Device> = host.input_devices().ok()?.collect();

    if !priorities.is_empty() {
        for want in priorities {
            if let Some(dev) = available.iter().find(|d| {
                d.description()
                    .map(|description| description_matches_priority(&description, want))
                    .unwrap_or(false)
            }) {
                if should_log {
                    println!("🎙️  Using preferred device: {}", want);
                }
                return Some(dev.clone());
            }
        }
    }

    // 4) 見つからなければデフォルト
    if should_log {
        println!("⚠️  No preferred device found, falling back to default input device");
    }
    host.default_input_device()
}

fn device_cache_key(device: &Device) -> String {
    device
        .description()
        .map(|description| description.to_string())
        .or_else(|_| device.id().map(|id| id.to_string()))
        .unwrap_or_else(|_| "<unknown-device>".to_string())
}

fn select_input_device_key(host: &cpal::Host, priorities: &[String]) -> Option<String> {
    let device = select_input_device_with_priorities(host, priorities, false)?;
    Some(device_cache_key(&device))
}

fn input_setup_matches_current_selection(cached: &CachedInputSetup) -> bool {
    let current_priorities = input_device_priorities();
    if should_revalidate_input_setup(
        &cached.input_device_priority,
        &current_priorities,
        *cached.last_validated_at.lock().unwrap(),
        Instant::now(),
    ) {
        let host = cpal::default_host();
        let Some(current_device_key) = select_input_device_key(&host, &current_priorities) else {
            return false;
        };

        let matches = current_device_key == cached.selected_device_key;
        if matches {
            *cached.last_validated_at.lock().unwrap() = Instant::now();
        }
        return matches;
    }

    true
}

fn should_revalidate_input_setup(
    cached_priorities: &[String],
    current_priorities: &[String],
    last_validated_at: Instant,
    now: Instant,
) -> bool {
    if current_priorities != cached_priorities {
        return true;
    }
    now.duration_since(last_validated_at) > INPUT_SETUP_REVALIDATION_INTERVAL
}

fn should_rebuild_input_stream(
    existing_identity: Option<&StreamIdentity>,
    desired_identity: &StreamIdentity,
    needs_rebuild: bool,
) -> bool {
    needs_rebuild || !matches!(existing_identity, Some(identity) if identity == desired_identity)
}

fn try_capture_buffer(
    recording: &AtomicBool,
    capture_generation: &AtomicU64,
    recording_state: &Arc<Mutex<Option<MemoryRecordingState>>>,
) -> Option<CaptureTarget> {
    if !recording.load(Ordering::SeqCst) {
        return None;
    }

    let (buffer, accepting_input, generation) = {
        let state = recording_state.lock().unwrap();
        let state = state.as_ref()?;
        (
            state.buffer.clone(),
            state.accepting_input.clone(),
            state.generation,
        )
    };

    if !accepting_input.load(Ordering::SeqCst)
        || generation != capture_generation.load(Ordering::SeqCst)
    {
        return None;
    }

    Some((buffer, accepting_input, generation))
}

fn append_input_i16(
    recording: &AtomicBool,
    capture_generation: &AtomicU64,
    recording_state: &Arc<Mutex<Option<MemoryRecordingState>>>,
    data: &[i16],
) {
    let Some((buffer, accepting_input, generation)) =
        try_capture_buffer(recording, capture_generation, recording_state)
    else {
        return;
    };

    let mut buf = buffer.lock().unwrap();
    if accepting_input.load(Ordering::SeqCst)
        && generation == capture_generation.load(Ordering::SeqCst)
    {
        buf.extend_from_slice(data);
    }
}

fn append_input_f32(
    recording: &AtomicBool,
    capture_generation: &AtomicU64,
    recording_state: &Arc<Mutex<Option<MemoryRecordingState>>>,
    data: &[f32],
) {
    let Some((buffer, accepting_input, generation)) =
        try_capture_buffer(recording, capture_generation, recording_state)
    else {
        return;
    };

    let mut buf = buffer.lock().unwrap();
    if accepting_input.load(Ordering::SeqCst)
        && generation == capture_generation.load(Ordering::SeqCst)
    {
        buf.extend(
            data.iter()
                .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16),
        );
    }
}

#[cfg(test)]
fn clear_input_setup_on_error<T, U, F>(
    cache: &InputSetupCache<U>,
    cleanup: F,
    result: Result<T, Box<dyn Error>>,
) -> Result<T, Box<dyn Error>>
where
    F: FnOnce(),
{
    if result.is_err() {
        cache.clear();
        cleanup();
    }
    result
}

#[cfg(test)]
fn run_start_recording<S, T, Resolve, Build, Play, Cleanup>(
    cache: &InputSetupCache<S>,
    validate_cached: impl FnOnce(&S) -> bool,
    resolve_input_setup: Resolve,
    build_stream: Build,
    play_stream: Play,
    cleanup_on_error: Cleanup,
) -> Result<T, Box<dyn Error>>
where
    S: Clone,
    Resolve: FnOnce() -> Result<S, Box<dyn Error>>,
    Build: FnOnce(&S) -> Result<T, Box<dyn Error>>,
    Play: FnOnce(&T) -> Result<(), Box<dyn Error>>,
    Cleanup: FnOnce(),
{
    let result = (|| {
        let input_setup = cache.get_or_try_init_if(validate_cached, resolve_input_setup)?;
        let stream = build_stream(&input_setup)?;
        play_stream(&stream)?;
        Ok(stream)
    })();

    clear_input_setup_on_error(cache, cleanup_on_error, result)
}

fn description_matches_priority(description: &DeviceDescription, wanted: &str) -> bool {
    description.name() == wanted || description.to_string() == wanted
}

fn device_list_label(description: &DeviceDescription) -> String {
    description.name().to_string()
}

// =============== WAVヘッダー生成機能 ================================
impl CpalAudioBackend {
    fn resolve_cached_input_setup(&self) -> Result<CachedInputSetup, Box<dyn Error>> {
        self.input_setup_cache
            .get_or_try_init_if(input_setup_matches_current_selection, || {
                let host = cpal::default_host();
                let input_device_priority = input_device_priorities();
                let device =
                    select_input_device_with_priorities(&host, &input_device_priority, true)
                        .ok_or(CpalBackendError::NoInputDevice)?;
                let supported_config = device.default_input_config()?;
                let stream_identity = StreamIdentity {
                    selected_device_key: device_cache_key(&device),
                    sample_format: supported_config.sample_format(),
                    sample_rate: supported_config.sample_rate(),
                    channels: supported_config.channels(),
                };
                Ok(CachedInputSetup {
                    selected_device_key: stream_identity.selected_device_key.clone(),
                    device,
                    supported_config,
                    input_device_priority,
                    last_validated_at: Arc::new(Mutex::new(Instant::now())),
                    stream_identity,
                })
            })
    }

    fn ensure_input_stream(&self) -> Result<CachedInputSetup, Box<dyn Error>> {
        let input_setup = self.resolve_cached_input_setup()?;
        let should_rebuild = {
            let stream = self.stream.lock().unwrap();
            should_rebuild_input_stream(
                stream.as_ref().map(|ready| &ready.identity),
                &input_setup.stream_identity,
                self.stream_needs_rebuild.load(Ordering::SeqCst),
            )
        };

        if should_rebuild {
            let sample_format = input_setup.supported_config.sample_format();
            let config: StreamConfig = input_setup.supported_config.clone().into();
            let stream_result = Self::build_memory_stream(
                self.recording.clone(),
                self.capture_generation.clone(),
                self.stream_needs_rebuild.clone(),
                self.recording_state.clone(),
                &input_setup.device,
                &config,
                sample_format,
            )
            .and_then(|stream| {
                stream.play()?;
                Ok(stream)
            });
            let stream = match stream_result {
                Ok(stream) => stream,
                Err(err) => {
                    self.input_setup_cache.value.lock().unwrap().take();
                    *self.stream.lock().unwrap() = None;
                    self.stream_needs_rebuild.store(true, Ordering::SeqCst);
                    return Err(err);
                }
            };
            *self.stream.lock().unwrap() = Some(ReadyInputStream {
                _stream: stream,
                identity: input_setup.stream_identity.clone(),
            });
            self.stream_needs_rebuild.store(false, Ordering::SeqCst);
        }

        Ok(input_setup)
    }

    pub fn warm_up(&self) -> Result<(), AudioBackendError> {
        self.ensure_input_stream()
            .map(|_| ())
            .map_err(|error| AudioBackendError::StreamOperation {
                message: error.to_string(),
            })
    }

    fn preferred_format() -> AudioFormat {
        match EnvConfig::get().audio.preferred_format {
            crate::utils::config::PreferredAudioFormat::Wav => AudioFormat::Wav,
            crate::utils::config::PreferredAudioFormat::Flac => AudioFormat::Flac,
        }
    }

    /// WAVファイルヘッダーを生成する
    ///
    /// # Arguments
    /// * `data_len` - PCMデータのバイト数
    /// * `sample_rate` - サンプルレート (例: 48000)
    /// * `channels` - チャンネル数 (例: 2)
    /// * `bits_per_sample` - サンプルあたりのビット数 (例: 16)
    ///
    /// # Returns
    /// 44バイトのWAVヘッダーデータ
    ///
    /// # Example
    /// ```
    /// use voice_input::infrastructure::audio::cpal_backend::CpalAudioBackend;
    ///
    /// // 1秒分のステレオ16bit 48kHzオーディオのヘッダー作成
    /// let data_len = 48000 * 2 * 2; // sample_rate * channels * bytes_per_sample
    /// let header = CpalAudioBackend::create_wav_header(data_len, 48000, 2, 16);
    /// assert_eq!(header.len(), 44);
    /// ```
    pub fn create_wav_header(
        data_len: u32,
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
    ) -> Vec<u8> {
        let mut header = Vec::with_capacity(44);

        // RIFF チャンク
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&(36 + data_len).to_le_bytes()); // ファイルサイズ - 8
        header.extend_from_slice(b"WAVE");

        // fmt チャンク
        header.extend_from_slice(b"fmt ");
        header.extend_from_slice(&16u32.to_le_bytes()); // fmtチャンクサイズ
        header.extend_from_slice(&1u16.to_le_bytes()); // PCMフォーマット
        header.extend_from_slice(&channels.to_le_bytes());
        header.extend_from_slice(&sample_rate.to_le_bytes());

        let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
        header.extend_from_slice(&byte_rate.to_le_bytes());

        let block_align = channels * bits_per_sample / 8;
        header.extend_from_slice(&block_align.to_le_bytes());
        header.extend_from_slice(&bits_per_sample.to_le_bytes());

        // data チャンク
        header.extend_from_slice(b"data");
        header.extend_from_slice(&data_len.to_le_bytes());

        header
    }

    /// PCMデータとWAVヘッダーを結合して完全なWAVデータを生成
    ///
    /// # Arguments
    /// * `pcm_data` - 音声のPCMデータ
    /// * `sample_rate` - サンプルレート
    /// * `channels` - チャンネル数
    ///
    /// # Returns
    /// 完全なWAVファイルデータ (ヘッダー + PCMデータ)
    ///
    /// # Errors
    /// - `AudioError::DataTooLarge` - データサイズが u32::MAX を超える場合
    ///
    /// # Example
    /// ```
    /// use voice_input::infrastructure::audio::cpal_backend::{CpalAudioBackend, Sample};
    ///
    /// // i16 サンプルの例
    /// let pcm_data: Vec<i16> = vec![0, 1000, -1000, 0];
    /// let wav_data = CpalAudioBackend::combine_wav_data(&pcm_data, 48000, 2).unwrap();
    /// assert_eq!(wav_data.len(), 44 + 8); // header + 4 samples * 2 bytes
    ///
    /// // f32 サンプルの例
    /// let pcm_data_f32: Vec<f32> = vec![0.0, 0.5, -0.5, 0.0];
    /// let wav_data_f32 = CpalAudioBackend::combine_wav_data(&pcm_data_f32, 44100, 1).unwrap();
    /// assert_eq!(wav_data_f32.len(), 44 + 8);
    /// ```
    pub fn combine_wav_data<T>(
        pcm_data: &[T],
        sample_rate: u32,
        channels: u16,
    ) -> Result<Vec<u8>, AudioError>
    where
        T: Sample + Copy,
    {
        // データサイズチェック（u32::MAX を超えないことを確認）
        let data_len = pcm_data.len() * 2; // 16bit = 2 bytes per sample
        if data_len > u32::MAX as usize {
            return Err(AudioError::DataTooLarge(data_len));
        }

        // WAVヘッダー生成
        let header = Self::create_wav_header(data_len as u32, sample_rate, channels, 16);

        // 結果バッファを事前確保（メモリ効率化）
        let mut wav_data = Vec::with_capacity(header.len() + data_len);
        wav_data.extend_from_slice(&header);

        // PCMデータをバイト列に変換して追加（追加アロケーションなし）
        for sample in pcm_data {
            let le = sample.as_pcm_le_bytes();
            wav_data.extend_from_slice(&le);
        }

        Ok(wav_data)
    }
}

// =============== 内部ユーティリティ ================================
impl CpalAudioBackend {
    const MIN_SILENCE_THRESHOLD: i32 = 500;
    const THRESHOLD_MULTIPLIER: f32 = 3.0;
    const NOISE_WINDOW_MS: u32 = 200;
    const MIN_SILENCE_DURATION_MS: u32 = 50;
    const MIN_RETAINED_FRAMES: usize = 1;

    /// メモリバッファのサイズ見積もり
    /// 録音時間に基づいて必要なバッファサイズを計算
    fn estimate_buffer_size(duration_secs: u32, sample_rate: u32, channels: u16) -> usize {
        // samples = sample_rate * channels * duration
        sample_rate as usize * channels as usize * duration_secs as usize
    }

    fn calculate_dynamic_threshold(samples: &[i16], sample_rate: u32, channels: u16) -> i16 {
        if samples.is_empty() {
            return Self::MIN_SILENCE_THRESHOLD as i16;
        }

        let frame_size = channels.max(1) as usize;
        let noise_window_frames =
            ((sample_rate as usize * Self::NOISE_WINDOW_MS as usize) / 1000).max(1);
        let noise_window_samples = noise_window_frames.saturating_mul(frame_size);
        let window_len = noise_window_samples.min(samples.len()).max(1);

        let sum_abs: i64 = samples[..window_len]
            .iter()
            .map(|&s| (s as i32).abs() as i64)
            .sum();
        let avg_abs = sum_abs / window_len as i64;

        let dynamic =
            ((avg_abs as f32) * Self::THRESHOLD_MULTIPLIER).max(Self::MIN_SILENCE_THRESHOLD as f32);
        dynamic
            .min(i16::MAX as f32)
            .round()
            .clamp(i16::MIN as f32, i16::MAX as f32) as i16
    }

    fn count_leading_silence_frames(samples: &[i16], frame_size: usize, threshold: i16) -> usize {
        let mut frames = 0;
        let total_frames = samples.len() / frame_size;

        while frames < total_frames {
            let frame = &samples[frames * frame_size..(frames + 1) * frame_size];
            let max = frame
                .iter()
                .map(|&s| (s as i32).abs())
                .max()
                .unwrap_or_default();
            if max > threshold as i32 {
                break;
            }
            frames += 1;
        }

        frames
    }

    fn count_trailing_silence_frames(samples: &[i16], frame_size: usize, threshold: i16) -> usize {
        let mut frames = 0;
        let total_frames = samples.len() / frame_size;

        while frames < total_frames {
            let frame = &samples
                [(total_frames - frames - 1) * frame_size..(total_frames - frames) * frame_size];
            let max = frame
                .iter()
                .map(|&s| (s as i32).abs())
                .max()
                .unwrap_or_default();
            if max > threshold as i32 {
                break;
            }
            frames += 1;
        }

        frames
    }

    fn min_silence_frames(sample_rate: u32) -> usize {
        ((sample_rate as usize * Self::MIN_SILENCE_DURATION_MS as usize) / 1000).max(1)
    }

    fn ensure_minimum_samples(samples: &[i16], frame_size: usize) -> Cow<'_, [i16]> {
        if samples.is_empty() {
            return Cow::Borrowed(samples);
        }

        let total_frames = samples.len() / frame_size;
        let retain_frames = Self::MIN_RETAINED_FRAMES.min(total_frames.max(1));
        let retain_samples = (retain_frames * frame_size).min(samples.len());

        if retain_samples == samples.len() {
            Cow::Borrowed(samples)
        } else {
            Cow::Owned(samples[..retain_samples].to_vec())
        }
    }

    fn trim_silence(samples: &[i16], sample_rate: u32, channels: u16) -> Cow<'_, [i16]> {
        if samples.is_empty() || channels == 0 {
            return Cow::Borrowed(samples);
        }

        let frame_size = channels as usize;
        let total_frames = samples.len() / frame_size;

        if total_frames == 0 {
            return Cow::Borrowed(samples);
        }

        let threshold = Self::calculate_dynamic_threshold(samples, sample_rate, channels);
        let min_silence_frames = Self::min_silence_frames(sample_rate);

        let leading = Self::count_leading_silence_frames(samples, frame_size, threshold);
        let trailing = Self::count_trailing_silence_frames(samples, frame_size, threshold);

        let start_frame = if leading >= min_silence_frames {
            leading.min(total_frames)
        } else {
            0
        };

        let end_frame = if trailing >= min_silence_frames {
            total_frames.saturating_sub(trailing)
        } else {
            total_frames
        };

        if start_frame == 0 && end_frame == total_frames {
            return Cow::Borrowed(samples);
        }

        if end_frame <= start_frame {
            return Self::ensure_minimum_samples(samples, frame_size);
        }

        let start_idx = start_frame * frame_size;
        let end_idx = end_frame * frame_size;

        if start_idx >= end_idx {
            return Self::ensure_minimum_samples(samples, frame_size);
        }

        Cow::Owned(samples[start_idx..end_idx].to_vec())
    }

    fn downmix_to_mono(samples: &[i16], channels: u16) -> Vec<i16> {
        let channels = channels as usize;
        if channels <= 1 {
            return samples.to_vec();
        }

        // フレームごとに平均してモノラル化する（ステレオ/多ch対応）
        let mut mono = Vec::with_capacity(samples.len() / channels + 1);
        let mut iter = samples.chunks_exact(channels);

        for frame in &mut iter {
            let sum: i32 = frame.iter().map(|&s| s as i32).sum();
            let avg = sum / channels as i32;
            mono.push(avg.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
        }

        // 端数フレームがある場合は平均して最後の1サンプルにまとめる
        let remainder = iter.remainder();
        if !remainder.is_empty() {
            let sum: i32 = remainder.iter().map(|&s| s as i32).sum();
            let avg = sum / remainder.len() as i32;
            mono.push(avg.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
        }

        mono
    }

    fn resample_to_16khz(
        samples: &[i16],
        sample_rate: u32,
    ) -> Result<ResampleOutcome, AudioResampleError> {
        if sample_rate == TARGET_SAMPLE_RATE {
            return Ok(ResampleOutcome {
                samples: samples.to_vec(),
                sample_rate,
            });
        }

        if samples.is_empty() {
            return Ok(ResampleOutcome {
                samples: Vec::new(),
                sample_rate,
            });
        }

        if samples.len() < MIN_RESAMPLE_FRAMES {
            return Ok(ResampleOutcome {
                samples: samples.to_vec(),
                sample_rate,
            });
        }

        let input_frames = samples.len();
        let chunk_size = input_frames.min(1024);
        let resample_ratio = TARGET_SAMPLE_RATE as f64 / sample_rate as f64;
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };

        let mut resampler = Async::<f32>::new_sinc(
            resample_ratio,
            2.0,
            &params,
            chunk_size,
            1,
            FixedAsync::Input,
        )?;

        let input_f32: Vec<f32> = samples
            .iter()
            .map(|&s| {
                if s == i16::MIN {
                    -1.0
                } else {
                    s as f32 / i16::MAX as f32
                }
            })
            .collect();
        let input =
            audioadapter_buffers::direct::InterleavedSlice::new(&input_f32, 1, input_frames)?;
        let output_frames_capacity = resampler.process_all_needed_output_len(input_frames);
        let mut output = vec![0.0f32; output_frames_capacity];
        let mut output_adapter = audioadapter_buffers::direct::InterleavedSlice::new_mut(
            &mut output,
            1,
            output_frames_capacity,
        )?;
        let (_, output_frames) =
            resampler.process_all_into_buffer(&input, &mut output_adapter, input_frames, None)?;
        output.truncate(output_frames);

        let resampled = output
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();
        Ok(ResampleOutcome {
            samples: resampled,
            sample_rate: TARGET_SAMPLE_RATE,
        })
    }

    /// 利用可能な入力デバイス名を返すユーティリティ
    pub fn list_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|iter| {
                iter.filter_map(|d| {
                    d.description()
                        .ok()
                        .map(|description| device_list_label(&description))
                })
                .collect::<Vec<String>>()
            })
            .unwrap_or_default()
    }

    /// メモリモード用のストリーム構築
    fn build_memory_stream(
        recording: Arc<AtomicBool>,
        capture_generation: Arc<AtomicU64>,
        stream_needs_rebuild: Arc<AtomicBool>,
        recording_state: Arc<Mutex<Option<MemoryRecordingState>>>,
        device: &Device,
        config: &StreamConfig,
        sample_format: SampleFormat,
    ) -> Result<Stream, Box<dyn Error>> {
        let stream = match sample_format {
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _| {
                    append_input_i16(
                        recording.as_ref(),
                        capture_generation.as_ref(),
                        &recording_state,
                        data,
                    );
                },
                move |e| {
                    stream_needs_rebuild.store(true, Ordering::SeqCst);
                    eprintln!("stream error: {e}");
                },
                None,
            )?,
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _| {
                    append_input_f32(
                        recording.as_ref(),
                        capture_generation.as_ref(),
                        &recording_state,
                        data,
                    );
                },
                move |e| {
                    stream_needs_rebuild.store(true, Ordering::SeqCst);
                    eprintln!("stream error: {e}");
                },
                None,
            )?,
            _ => return Err(CpalBackendError::UnsupportedSampleFormat.into()),
        };

        Ok(stream)
    }
}

impl AudioBackend for CpalAudioBackend {
    /// 録音ストリームを開始します。
    fn start_recording(&self) -> Result<(), AudioBackendError> {
        if self.is_recording() {
            return Err(CpalBackendError::AlreadyRecording.into());
        }

        let input_setup =
            self.ensure_input_stream()
                .map_err(|error| AudioBackendError::StreamOperation {
                    message: error.to_string(),
                })?;
        let config: StreamConfig = input_setup.supported_config.clone().into();
        let sample_rate = config.sample_rate;
        let channels = config.channels;
        let capacity = Self::estimate_buffer_size(30, sample_rate, channels);
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(capacity)));
        let generation = self.capture_generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self.recording_state.lock().unwrap() = Some(MemoryRecordingState {
            buffer,
            sample_rate,
            channels,
            generation,
            accepting_input: Arc::new(AtomicBool::new(true)),
        });
        self.recording.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// 録音を停止し、音声データを返します。
    fn stop_recording(&self) -> Result<AudioData, AudioBackendError> {
        let overall_timer = profiling::Timer::start("audio.stop_recording");
        if !self.is_recording() {
            return Err(CpalBackendError::NotRecording.into());
        }

        self.recording.store(false, Ordering::SeqCst);
        let retired_generation = self.capture_generation.fetch_add(1, Ordering::SeqCst);

        // RecordingStateを取得
        let state = self
            .recording_state
            .lock()
            .unwrap()
            .take()
            .ok_or(CpalBackendError::RecordingStateNotSet)?;
        if state.generation != retired_generation {
            return Err(CpalBackendError::RecordingStateNotSet.into());
        }
        state.accepting_input.store(false, Ordering::SeqCst);

        // メモリモード: バッファからエンコード（既定: FLAC）
        let samples = state.buffer.lock().unwrap();
        let samples_len = samples.len();
        let trim_timer = profiling::Timer::start("audio.trim_silence");
        let trimmed = Self::trim_silence(&samples, state.sample_rate, state.channels);
        if profiling::enabled() {
            trim_timer.log_with(&format!(
                "samples={} trimmed={} rate={} ch={}",
                samples_len,
                trimmed.len(),
                state.sample_rate,
                state.channels
            ));
        } else {
            trim_timer.log();
        }

        // エンコード前にモノラル化して送信サイズを減らす
        let mut processed = if state.channels > 1 {
            let mono = Self::downmix_to_mono(trimmed.as_ref(), state.channels);
            ProcessedAudio {
                samples: Cow::Owned(mono),
                sample_rate: state.sample_rate,
                channels: 1,
            }
        } else {
            ProcessedAudio {
                samples: trimmed,
                sample_rate: state.sample_rate,
                channels: state.channels,
            }
        };

        if processed.sample_rate != TARGET_SAMPLE_RATE {
            let resample_timer = profiling::Timer::start("audio.resample_16khz");
            let resampled = Self::resample_to_16khz(&processed.samples, processed.sample_rate)
                .map_err(|error| AudioBackendError::Processing {
                    message: error.to_string(),
                })?;
            processed = ProcessedAudio {
                samples: Cow::Owned(resampled.samples),
                sample_rate: resampled.sample_rate,
                channels: processed.channels,
            };
            resample_timer.log();
        }

        let result = match Self::preferred_format() {
            AudioFormat::Flac => {
                let encode_timer = profiling::Timer::start("audio.encode_flac");
                match encoder::flac::encode_flac_i16(
                    &processed.samples,
                    processed.sample_rate,
                    processed.channels,
                ) {
                    Ok(flac) => {
                        if profiling::enabled() {
                            encode_timer.log_with(&format!("bytes={}", flac.len()));
                        } else {
                            encode_timer.log();
                        }
                        Ok(AudioData {
                            bytes: flac,
                            mime_type: "audio/flac",
                            file_name: "audio.flac".to_string(),
                        })
                    }
                    Err(e) => {
                        encode_timer.log();
                        eprintln!("FLAC encode failed (fallback to WAV): {}", e);
                        profiling::log_point("audio.encode_flac.error", "fallback=wav");
                        let wav = Self::combine_wav_data(
                            &processed.samples,
                            processed.sample_rate,
                            processed.channels,
                        )?;
                        Ok(AudioData {
                            bytes: wav,
                            mime_type: "audio/wav",
                            file_name: "audio.wav".to_string(),
                        })
                    }
                }
            }
            AudioFormat::Wav => {
                let encode_timer = profiling::Timer::start("audio.encode_wav");
                let wav = Self::combine_wav_data(
                    &processed.samples,
                    processed.sample_rate,
                    processed.channels,
                )?;
                if profiling::enabled() {
                    encode_timer.log_with(&format!("bytes={}", wav.len()));
                } else {
                    encode_timer.log();
                }
                Ok(AudioData {
                    bytes: wav,
                    mime_type: "audio/wav",
                    file_name: "audio.wav".to_string(),
                })
            }
        };

        if profiling::enabled() {
            if let Ok(data) = result.as_ref() {
                profiling::log_point(
                    "audio.converted_size",
                    &format!("bytes={}", data.bytes.len()),
                );
            }
        }

        if profiling::enabled() {
            match result.as_ref() {
                Ok(data) => overall_timer.log_with(&format!(
                    "bytes={} mime={}",
                    data.bytes.len(),
                    data.mime_type
                )),
                Err(_) => overall_timer.log(),
            }
        } else {
            overall_timer.log();
        }

        result
    }

    /// 録音中かどうかを確認します。
    fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     /// `INPUT_DEVICE_PRIORITY` が参照されているかをエラーメッセージで確認。
//     #[test]
//     fn input_device_priority_env_is_respected_in_error() {
//         unsafe { std::env::set_var("INPUT_DEVICE_PRIORITY", "ClearlyNonexistentDevice") };
//         let backend = CpalAudioBackend::default();
//         let err = backend
//             .start_recording()
//             .expect_err("should fail without device");
//         assert!(err.to_string().contains("INPUT_DEVICE_PRIORITY"));
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::{DeviceDescriptionBuilder, DeviceType, InterfaceType};

    /// `INPUT_DEVICE_PRIORITY` に存在しないデバイスを設定し、バックエンドが
    /// (1) フォールバックを介して開始する **または** (2) 入力デバイスの欠落に
    /// 言及するエラーを返すことを確認します。これにより、優先順位/フォールバック
    /// コードが誤って削除されることを防ぎます。
    use std::sync::Mutex;

    static INPUT_DEVICE_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn init_env_config_for_test() {
        let _ = crate::utils::config::EnvConfig::init();
    }

    /// キャッシュされた入力設定は明示的に破棄されるまで再利用される
    #[test]
    fn input_setup_cache_reuses_resolved_value_until_cleared() {
        let cache = InputSetupCache::new();
        let resolve_count = Arc::new(Mutex::new(0usize));

        let first = cache
            .get_or_try_init({
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(41usize)
                }
            })
            .unwrap();
        let second = cache
            .get_or_try_init({
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(99usize)
                }
            })
            .unwrap();

        assert_eq!(first, 41);
        assert_eq!(second, 41);
        assert_eq!(*resolve_count.lock().unwrap(), 1);
    }

    /// キャッシュを破棄すると次回は設定を再解決する
    #[test]
    fn input_setup_cache_reloads_after_clear() {
        let cache = InputSetupCache::new();
        let resolve_count = Arc::new(Mutex::new(0usize));

        let first = cache
            .get_or_try_init({
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(7usize)
                }
            })
            .unwrap();
        cache.clear();
        let second = cache
            .get_or_try_init({
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(8usize)
                }
            })
            .unwrap();

        assert_eq!(first, 7);
        assert_eq!(second, 8);
        assert_eq!(*resolve_count.lock().unwrap(), 2);
    }

    /// キャッシュ済み設定が無効化された場合は再解決される
    #[test]
    fn input_setup_cache_reloads_when_cached_value_is_invalid() {
        let cache = InputSetupCache::new();
        let resolve_count = Arc::new(Mutex::new(0usize));

        let first = cache
            .get_or_try_init_if(|_| true, {
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(11usize)
                }
            })
            .unwrap();
        let second = cache
            .get_or_try_init_if(|_| false, {
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(12usize)
                }
            })
            .unwrap();

        assert_eq!(first, 11);
        assert_eq!(second, 12);
        assert_eq!(*resolve_count.lock().unwrap(), 2);
    }

    /// 利用処理が失敗したらキャッシュと録音状態の巻き戻しが行われる
    #[test]
    fn clear_input_setup_on_error_rolls_back_cache_and_cleanup() {
        let cache = InputSetupCache::new();
        cache.get_or_try_init(|| Ok::<_, ()>(3usize)).unwrap();
        let cleanup_called = Arc::new(AtomicBool::new(false));
        let resolve_count = Arc::new(Mutex::new(0usize));

        let result = clear_input_setup_on_error(
            &cache,
            {
                let cleanup_called = cleanup_called.clone();
                move || cleanup_called.store(true, Ordering::SeqCst)
            },
            Err::<(), Box<dyn Error>>(CpalBackendError::NoInputDevice.into()),
        );

        assert!(result.is_err());
        assert!(cleanup_called.load(Ordering::SeqCst));
        let reloaded = cache
            .get_or_try_init({
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(8usize)
                }
            })
            .unwrap();
        assert_eq!(reloaded, 8);
        assert_eq!(*resolve_count.lock().unwrap(), 1);
    }

    /// 利用処理が成功したらキャッシュも録音状態も維持される
    #[test]
    fn clear_input_setup_on_success_keeps_cache_and_skips_cleanup() {
        let cache = InputSetupCache::new();
        cache.get_or_try_init(|| Ok::<_, ()>(5usize)).unwrap();
        let cleanup_called = Arc::new(AtomicBool::new(false));
        let resolve_count = Arc::new(Mutex::new(0usize));

        let result = clear_input_setup_on_error(
            &cache,
            {
                let cleanup_called = cleanup_called.clone();
                move || cleanup_called.store(true, Ordering::SeqCst)
            },
            Ok::<usize, Box<dyn Error>>(9usize),
        )
        .unwrap();

        assert_eq!(result, 9);
        assert!(!cleanup_called.load(Ordering::SeqCst));
        let cached = cache
            .get_or_try_init({
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(6usize)
                }
            })
            .unwrap();
        assert_eq!(cached, 5);
        assert_eq!(*resolve_count.lock().unwrap(), 0);
    }

    /// start ワークフローはキャッシュ有効時に入力設定の再解決を避ける
    #[test]
    fn run_start_recording_skips_resolve_when_cached_value_is_valid() {
        let cache = InputSetupCache::new();
        let resolve_count = Arc::new(Mutex::new(0usize));

        let first = run_start_recording(
            &cache,
            |_| true,
            {
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, Box<dyn Error>>(21usize)
                }
            },
            |setup| Ok::<_, Box<dyn Error>>(*setup + 1),
            |_| Ok::<_, Box<dyn Error>>(()),
            || {},
        )
        .unwrap();
        let second = run_start_recording(
            &cache,
            |_| true,
            {
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, Box<dyn Error>>(99usize)
                }
            },
            |setup| Ok::<_, Box<dyn Error>>(*setup + 1),
            |_| Ok::<_, Box<dyn Error>>(()),
            || {},
        )
        .unwrap();

        assert_eq!(first, 22);
        assert_eq!(second, 22);
        assert_eq!(*resolve_count.lock().unwrap(), 1);
    }

    /// start ワークフローはキャッシュ無効時に入力設定を再解決する
    #[test]
    fn run_start_recording_reloads_when_cached_value_is_invalid() {
        let cache = InputSetupCache::new();
        let resolve_count = Arc::new(Mutex::new(0usize));

        let first = run_start_recording(
            &cache,
            |_| true,
            {
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, Box<dyn Error>>(31usize)
                }
            },
            |setup| Ok::<_, Box<dyn Error>>(*setup),
            |_| Ok::<_, Box<dyn Error>>(()),
            || {},
        )
        .unwrap();
        let second = run_start_recording(
            &cache,
            |_| false,
            {
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, Box<dyn Error>>(32usize)
                }
            },
            |setup| Ok::<_, Box<dyn Error>>(*setup),
            |_| Ok::<_, Box<dyn Error>>(()),
            || {},
        )
        .unwrap();

        assert_eq!(first, 31);
        assert_eq!(second, 32);
        assert_eq!(*resolve_count.lock().unwrap(), 2);
    }

    /// 同じ優先順位で直近に検証済みなら入力設定の再検証を省略する
    #[test]
    fn recent_input_setup_validation_skips_revalidation() {
        let priorities = vec!["AT2040USB".to_string()];
        let now = Instant::now();

        let should_revalidate = should_revalidate_input_setup(
            &priorities,
            &priorities,
            now,
            now + Duration::from_millis(500),
        );

        assert!(!should_revalidate);
    }

    /// 優先順位が変わったら直近の検証時刻に関係なく再検証する
    #[test]
    fn changed_priorities_force_input_setup_revalidation() {
        let cached = vec!["AT2040USB".to_string()];
        let current = vec!["MacBook Pro Microphone".to_string()];
        let now = Instant::now();

        let should_revalidate =
            should_revalidate_input_setup(&cached, &current, now, now + Duration::from_millis(500));

        assert!(should_revalidate);
    }

    /// 同じ優先順位でも一定時間を過ぎたら入力設定を再検証する
    #[test]
    fn stale_input_setup_validation_revalidates() {
        let priorities = vec!["AT2040USB".to_string()];
        let now = Instant::now();

        let should_revalidate = should_revalidate_input_setup(
            &priorities,
            &priorities,
            now,
            now + INPUT_SETUP_REVALIDATION_INTERVAL + Duration::from_millis(1),
        );

        assert!(should_revalidate);
    }

    /// 同じデバイス設定なら既存ストリームを再利用する
    #[test]
    fn matching_stream_identity_skips_rebuild() {
        let identity = StreamIdentity {
            selected_device_key: "AT2040USB".to_string(),
            sample_format: SampleFormat::F32,
            sample_rate: 48_000,
            channels: 1,
        };

        let should_rebuild = should_rebuild_input_stream(Some(&identity), &identity, false);

        assert!(!should_rebuild);
    }

    /// デバイス設定が変わったらストリームを張り直す
    #[test]
    fn changed_stream_identity_triggers_rebuild() {
        let existing = StreamIdentity {
            selected_device_key: "AT2040USB".to_string(),
            sample_format: SampleFormat::F32,
            sample_rate: 48_000,
            channels: 1,
        };
        let desired = StreamIdentity {
            selected_device_key: "MacBook Pro Microphone".to_string(),
            sample_format: SampleFormat::F32,
            sample_rate: 48_000,
            channels: 1,
        };

        let should_rebuild = should_rebuild_input_stream(Some(&existing), &desired, false);

        assert!(should_rebuild);
    }

    /// stream error が立っている場合は同じ設定でも張り直す
    #[test]
    fn unhealthy_stream_identity_triggers_rebuild() {
        let identity = StreamIdentity {
            selected_device_key: "AT2040USB".to_string(),
            sample_format: SampleFormat::F32,
            sample_rate: 48_000,
            channels: 1,
        };

        let should_rebuild = should_rebuild_input_stream(Some(&identity), &identity, true);

        assert!(should_rebuild);
    }

    /// 世代が切り替わった callback は停止後の buffer に追記しない
    #[test]
    fn stale_generation_does_not_append_after_stop() {
        let recording = AtomicBool::new(true);
        let capture_generation = AtomicU64::new(2);
        let buffer = Arc::new(Mutex::new(vec![10i16]));
        let accepting_input = Arc::new(AtomicBool::new(false));
        let recording_state = Arc::new(Mutex::new(Some(MemoryRecordingState {
            buffer: buffer.clone(),
            sample_rate: 48_000,
            channels: 1,
            generation: 1,
            accepting_input,
        })));

        append_input_i16(&recording, &capture_generation, &recording_state, &[20, 30]);

        assert_eq!(*buffer.lock().unwrap(), vec![10i16]);
    }

    /// stream 構築失敗時はキャッシュを破棄して cleanup を実行する
    #[test]
    fn run_start_recording_clears_cache_when_build_fails() {
        let cache = InputSetupCache::new();
        let cleanup_called = Arc::new(AtomicBool::new(false));
        let resolve_count = Arc::new(Mutex::new(0usize));

        let result = run_start_recording(
            &cache,
            |_| true,
            {
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, Box<dyn Error>>(41usize)
                }
            },
            |_| Err::<usize, Box<dyn Error>>(CpalBackendError::NoInputDevice.into()),
            |_| Ok::<_, Box<dyn Error>>(()),
            {
                let cleanup_called = cleanup_called.clone();
                move || cleanup_called.store(true, Ordering::SeqCst)
            },
        );

        assert!(result.is_err());
        assert!(cleanup_called.load(Ordering::SeqCst));

        let reloaded = cache
            .get_or_try_init({
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(42usize)
                }
            })
            .unwrap();
        assert_eq!(reloaded, 42);
        assert_eq!(*resolve_count.lock().unwrap(), 2);
    }

    /// stream 再生失敗時はキャッシュを破棄して cleanup を実行する
    #[test]
    fn run_start_recording_clears_cache_when_play_fails() {
        let cache = InputSetupCache::new();
        let cleanup_called = Arc::new(AtomicBool::new(false));
        let resolve_count = Arc::new(Mutex::new(0usize));

        let result = run_start_recording(
            &cache,
            |_| true,
            {
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, Box<dyn Error>>(51usize)
                }
            },
            |setup| Ok::<_, Box<dyn Error>>(*setup + 1),
            |_| Err::<(), Box<dyn Error>>(CpalBackendError::NoInputDevice.into()),
            {
                let cleanup_called = cleanup_called.clone();
                move || cleanup_called.store(true, Ordering::SeqCst)
            },
        );

        assert!(result.is_err());
        assert!(cleanup_called.load(Ordering::SeqCst));

        let reloaded = cache
            .get_or_try_init({
                let resolve_count = resolve_count.clone();
                move || {
                    *resolve_count.lock().unwrap() += 1;
                    Ok::<_, ()>(52usize)
                }
            })
            .unwrap();
        assert_eq!(reloaded, 52);
        assert_eq!(*resolve_count.lock().unwrap(), 2);
    }

    /// 列挙したデバイス名を優先順位設定へそのまま利用できる
    #[test]
    fn listed_device_name_matches_priority_key() {
        let description = DeviceDescriptionBuilder::new("Built-in Microphone")
            .manufacturer("Apple")
            .device_type(DeviceType::Microphone)
            .interface_type(InterfaceType::BuiltIn)
            .build();

        assert_eq!(device_list_label(&description), "Built-in Microphone");
        assert!(description_matches_priority(
            &description,
            "Built-in Microphone"
        ));
    }

    /// 詳細表示文字列も優先順位設定で受け入れられる
    #[test]
    fn detailed_device_description_also_matches_priority_key() {
        let description = DeviceDescriptionBuilder::new("USB Mic")
            .manufacturer("Acme")
            .device_type(DeviceType::Microphone)
            .interface_type(InterfaceType::Usb)
            .build();

        let detailed = description.to_string();
        assert_ne!(detailed, description.name());
        assert!(description_matches_priority(&description, &detailed));
    }

    /// 入力デバイス優先順位の環境変数が考慮される
    #[test]
    fn input_device_priority_env_is_respected() {
        let _guard = INPUT_DEVICE_ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("INPUT_DEVICE_PRIORITY", "ClearlyNonexistentDevice") };

        let backend = CpalAudioBackend::default();
        let result = backend.start_recording();

        // 環境変数を元に戻す（テスト間影響防止）。
        unsafe { std::env::remove_var("INPUT_DEVICE_PRIORITY") };

        match result {
            Ok(_) => {
                // Fallback device found → recording started
                assert!(backend.is_recording());
                backend.stop_recording().unwrap();
            }
            Err(e) => {
                // Headless / CI environment without any devices
                let msg = e.to_string();
                assert!(
                    msg.contains("INPUT_DEVICE_PRIORITY")
                        || msg.contains("no input device")
                        || msg.contains("no longer available")
                        || msg.contains("unknown error"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    /// WAVヘッダーがRIFF/format/data構造を満たす
    #[test]
    fn wav_header_has_expected_structure() {
        // 1秒のステレオ16bit 48kHzオーディオ
        let data_len = 48000 * 2 * 2; // sample_rate * channels * bytes_per_sample
        let header = CpalAudioBackend::create_wav_header(data_len, 48000, 2, 16);

        // ヘッダーサイズは44バイト
        assert_eq!(header.len(), 44);

        // RIFFチャンクの検証
        assert_eq!(&header[0..4], b"RIFF");
        let file_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        assert_eq!(file_size, 36 + data_len);
        assert_eq!(&header[8..12], b"WAVE");

        // fmtチャンクの検証
        assert_eq!(&header[12..16], b"fmt ");
        let fmt_size = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
        assert_eq!(fmt_size, 16);
        let format = u16::from_le_bytes([header[20], header[21]]);
        assert_eq!(format, 1); // PCMフォーマット
        let channels = u16::from_le_bytes([header[22], header[23]]);
        assert_eq!(channels, 2);
        let sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
        assert_eq!(sample_rate, 48000);
        let byte_rate = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);
        assert_eq!(byte_rate, 48000 * 2 * 2); // 192000
        let block_align = u16::from_le_bytes([header[32], header[33]]);
        assert_eq!(block_align, 4); // 2 channels * 16 bits / 8
        let bits_per_sample = u16::from_le_bytes([header[34], header[35]]);
        assert_eq!(bits_per_sample, 16);

        // dataチャンクの検証
        assert_eq!(&header[36..40], b"data");
        let data_size = u32::from_le_bytes([header[40], header[41], header[42], header[43]]);
        assert_eq!(data_size, data_len);
    }

    /// モノラル設定のWAVヘッダーが正しい
    #[test]
    fn wav_header_supports_mono() {
        // モノラル設定でのヘッダー生成
        let data_len = 44100 * 2; // 44.1kHz, mono, 16bit
        let header = CpalAudioBackend::create_wav_header(data_len, 44100, 1, 16);

        assert_eq!(header.len(), 44);

        // チャンネル数確認
        let channels = u16::from_le_bytes([header[22], header[23]]);
        assert_eq!(channels, 1);

        // バイトレート確認
        let byte_rate = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);
        assert_eq!(byte_rate, 44100 * 2); // 88200

        // ブロックアライン確認
        let block_align = u16::from_le_bytes([header[32], header[33]]);
        assert_eq!(block_align, 2); // 1 channel * 16 bits / 8
    }

    /// サンプルレートがヘッダーに正しく反映される
    #[test]
    fn wav_header_reflects_sample_rate() {
        let sample_rates = vec![8000, 16000, 22050, 44100, 48000, 96000];

        for rate in sample_rates {
            let data_len = rate * 2 * 2; // 1秒分のステレオ16bit
            let header = CpalAudioBackend::create_wav_header(data_len, rate, 2, 16);

            let header_sample_rate =
                u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
            assert_eq!(header_sample_rate, rate);

            let byte_rate = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);
            assert_eq!(byte_rate, rate * 2 * 2);
        }
    }

    /// データ長0でもWAVヘッダーを生成できる
    #[test]
    fn wav_header_allows_empty_data() {
        // データ長0でのヘッダー生成
        let header = CpalAudioBackend::create_wav_header(0, 48000, 2, 16);

        assert_eq!(header.len(), 44);

        // ファイルサイズは36（ヘッダー44 - 8）
        let file_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        assert_eq!(file_size, 36);

        // データサイズは0
        let data_size = u32::from_le_bytes([header[40], header[41], header[42], header[43]]);
        assert_eq!(data_size, 0);
    }

    /// i16サンプルの変換が正しい
    #[test]
    fn sample_trait_handles_i16() {
        // i16 のサンプル変換テスト
        let sample: i16 = 1000;
        assert_eq!(sample.to_i16(), 1000);
        assert_eq!(sample.to_le_bytes(), [0xE8, 0x03]); // 1000 in little endian

        let sample: i16 = -1000;
        assert_eq!(sample.to_i16(), -1000);
        assert_eq!(sample.to_le_bytes(), [0x18, 0xFC]); // -1000 in little endian

        let sample: i16 = 0;
        assert_eq!(sample.to_i16(), 0);
        assert_eq!(sample.to_le_bytes(), [0x00, 0x00]);
    }

    /// f32サンプルの変換が正しい
    #[test]
    fn sample_trait_handles_f32() {
        // f32 のサンプル変換テスト

        // 正確な値のテスト
        let sample: f32 = 1.0;
        assert_eq!(sample.to_i16(), i16::MAX);

        let sample: f32 = -1.0;
        assert_eq!(sample.to_i16(), i16::MIN + 1); // -32767 (not -32768 due to rounding)

        let sample: f32 = 0.0;
        assert_eq!(sample.to_i16(), 0);

        // クリッピングのテスト
        let sample: f32 = 1.5;
        assert_eq!(sample.to_i16(), i16::MAX); // クリップされる

        let sample: f32 = -1.5;
        assert_eq!(sample.to_i16(), i16::MIN + 1); // クリップされる

        // 中間値のテスト
        let sample: f32 = 0.5;
        assert_eq!(sample.to_i16(), 16383); // ≈ i16::MAX / 2

        let sample: f32 = -0.5;
        assert_eq!(sample.to_i16(), -16383); // ≈ i16::MIN / 2
    }

    /// f32サンプルがPCMのリトルエンディアンに変換される
    #[test]
    fn sample_f32_converts_to_le_bytes() {
        // f32 -> bytes 変換テスト
        let sample: f32 = 0.0;
        assert_eq!(Sample::as_pcm_le_bytes(&sample), [0x00, 0x00]);

        let sample: f32 = 1.0;
        let bytes = Sample::as_pcm_le_bytes(&sample);
        let reconstructed = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(reconstructed, i16::MAX);
    }

    /// i16データをWAVに結合できる
    #[test]
    fn combine_wav_data_from_i16() {
        // i16データの結合テスト
        let pcm_data: Vec<i16> = vec![100, -100, 1000, -1000, 0];
        let result = CpalAudioBackend::combine_wav_data(&pcm_data, 48000, 2).unwrap();

        // ヘッダー(44バイト) + データ(5サンプル * 2バイト = 10バイト)
        assert_eq!(result.len(), 44 + 10);

        // ヘッダーの検証
        assert_eq!(&result[0..4], b"RIFF");
        assert_eq!(&result[8..12], b"WAVE");

        // データ部分の検証（リトルエンディアン）
        assert_eq!(&result[44..46], &[100u8, 0]); // 100
        assert_eq!(&result[46..48], &[156u8, 255]); // -100
        assert_eq!(&result[48..50], &[232u8, 3]); // 1000
        assert_eq!(&result[50..52], &[24u8, 252]); // -1000
        assert_eq!(&result[52..54], &[0u8, 0]); // 0
    }

    /// f32データをWAVに結合できる
    #[test]
    fn combine_wav_data_from_f32() {
        // f32データの結合テスト
        let pcm_data: Vec<f32> = vec![0.0, 1.0, -1.0, 0.5, -0.5];
        let result = CpalAudioBackend::combine_wav_data(&pcm_data, 44100, 1).unwrap();

        // ヘッダー(44バイト) + データ(5サンプル * 2バイト = 10バイト)
        assert_eq!(result.len(), 44 + 10);

        // サンプルレートとチャンネル数の確認
        let sample_rate = u32::from_le_bytes([result[24], result[25], result[26], result[27]]);
        assert_eq!(sample_rate, 44100);
        let channels = u16::from_le_bytes([result[22], result[23]]);
        assert_eq!(channels, 1);

        // データ部分の検証
        let sample1 = i16::from_le_bytes([result[44], result[45]]);
        assert_eq!(sample1, 0); // 0.0 -> 0

        let sample2 = i16::from_le_bytes([result[46], result[47]]);
        assert_eq!(sample2, i16::MAX); // 1.0 -> 32767

        let sample3 = i16::from_le_bytes([result[48], result[49]]);
        assert_eq!(sample3, i16::MIN + 1); // -1.0 -> -32767
    }

    /// 空のPCMデータでもWAVを生成できる
    #[test]
    fn combine_wav_data_with_empty_pcm() {
        // 空のPCMデータ
        let pcm_data: Vec<i16> = vec![];
        let result = CpalAudioBackend::combine_wav_data(&pcm_data, 48000, 2).unwrap();

        // ヘッダーのみ
        assert_eq!(result.len(), 44);

        // データサイズは0
        let data_size = u32::from_le_bytes([result[40], result[41], result[42], result[43]]);
        assert_eq!(data_size, 0);
    }

    /// ステレオデータのインターリーブが保たれる
    #[test]
    fn combine_wav_data_preserves_stereo_interleaving() {
        // ステレオデータのインターリーブ確認
        // 左チャンネル: 100, 200
        // 右チャンネル: -100, -200
        // インターリーブ後: 100, -100, 200, -200
        let pcm_data: Vec<i16> = vec![100, -100, 200, -200];
        let result = CpalAudioBackend::combine_wav_data(&pcm_data, 48000, 2).unwrap();

        assert_eq!(result.len(), 44 + 8); // 4サンプル * 2バイト

        // チャンネル数確認
        let channels = u16::from_le_bytes([result[22], result[23]]);
        assert_eq!(channels, 2);

        // データ確認
        assert_eq!(&result[44..46], &[100u8, 0]); // L: 100
        assert_eq!(&result[46..48], &[156u8, 255]); // R: -100
        assert_eq!(&result[48..50], &[200u8, 0]); // L: 200
        assert_eq!(&result[50..52], &[56u8, 255]); // R: -200
    }

    /// バックエンド初期状態で録音は開始されていない
    #[test]
    fn backend_starts_idle_in_memory_mode() {
        // メモリモード専用になったことを確認
        let backend = CpalAudioBackend::default();

        // 録音状態は初期状態でNone
        assert!(backend.recording_state.lock().unwrap().is_none());

        // 録音中でない
        assert!(!backend.is_recording());
    }

    /// AudioDataがclone/debug/bytesアクセスに対応する
    #[test]
    fn audio_data_struct_supports_clone_and_debug() {
        // Data creation
        let data = vec![1, 2, 3, 4, 5];
        let audio_data = AudioData {
            bytes: data.clone(),
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };

        // Data access
        assert_eq!(audio_data.bytes, data);

        // Debug trait
        assert!(format!("{:?}", audio_data).contains("AudioData"));

        // Clone trait
        let cloned = audio_data.clone();
        assert_eq!(cloned.bytes, data);
    }

    /// メモリ録音状態が正しく初期化される
    #[test]
    fn memory_recording_state_initializes() {
        // メモリモードの状態作成
        let buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
        let memory_state = MemoryRecordingState {
            buffer: buffer.clone(),
            sample_rate: 48000,
            channels: 2,
            generation: 1,
            accepting_input: Arc::new(AtomicBool::new(true)),
        };

        // bufferが適切に初期化されているか確認
        assert_eq!(memory_state.sample_rate, 48000);
        assert_eq!(memory_state.channels, 2);
        assert!(memory_state.buffer.lock().unwrap().is_empty());
    }

    /// recording_stateが初期状態でNoneである
    #[test]
    fn backend_starts_without_recording_state() {
        let backend = CpalAudioBackend::default();

        // 初期状態はNone
        assert!(backend.recording_state.lock().unwrap().is_none());

        // recording_stateフィールドが存在することを確認
        assert!(!backend.is_recording());
    }

    /// バッファサイズ見積もりが期待値と一致する
    #[test]
    fn estimate_buffer_size_matches_expected() {
        // 48kHz, 2ch, 1秒
        let size = CpalAudioBackend::estimate_buffer_size(1, 48000, 2);
        assert_eq!(size, 96000); // 48000 * 2 * 1

        // 44.1kHz, 1ch, 30秒
        let size = CpalAudioBackend::estimate_buffer_size(30, 44100, 1);
        assert_eq!(size, 1323000); // 44100 * 1 * 30

        // 48kHz, 2ch, 30秒（最大録音時間）
        let size = CpalAudioBackend::estimate_buffer_size(30, 48000, 2);
        assert_eq!(size, 2880000); // 48000 * 2 * 30
    }

    /// 録音開始前の初期状態を確認できる
    #[test]
    fn start_recording_initial_state_is_idle() {
        let backend = CpalAudioBackend::default();

        // 録音開始前の状態確認
        assert!(!backend.is_recording());
        assert!(backend.recording_state.lock().unwrap().is_none());

        // 注意: 実際のデバイスが必要なため、CI環境では失敗する可能性がある
        // ここではバックエンドの初期状態のみをテストする
    }

    /// バックエンド初期化時に録音/ストリームが空である
    #[test]
    fn backend_initial_state_has_no_stream_or_recording() {
        let backend = CpalAudioBackend::default();

        // 録音開始前の状態確認
        assert!(!backend.is_recording());
        assert!(backend.recording_state.lock().unwrap().is_none());

        // streamも初期状態でNone
        assert!(backend.stream.lock().unwrap().is_none());
    }

    /// メモリモード停止でFLACが返る
    #[test]
    fn stop_recording_returns_flac_in_memory_mode() {
        init_env_config_for_test();
        // メモリモードでの動作をシミュレート
        let backend = CpalAudioBackend::default();

        // テスト用のMemoryRecordingStateを設定
        let buffer = Arc::new(Mutex::new(vec![100i16, -100, 0, 1000, -1000]));
        *backend.recording_state.lock().unwrap() = Some(MemoryRecordingState {
            buffer: buffer.clone(),
            sample_rate: 48000,
            channels: 1,
            generation: 1,
            accepting_input: Arc::new(AtomicBool::new(true)),
        });
        backend.capture_generation.store(1, Ordering::SeqCst);

        // 録音フラグを設定
        backend.recording.store(true, Ordering::SeqCst);

        // stop_recordingを実行
        let result = backend.stop_recording().unwrap();

        // 既定はFLACで返る
        assert_eq!(result.mime_type, "audio/flac");
        assert_eq!(result.file_name, "audio.flac");
        assert!(result.bytes.len() > 4);
        assert_eq!(&result.bytes[0..4], b"fLaC");

        // 録音状態がクリアされていることを確認
        assert!(!backend.is_recording());
        assert!(backend.recording_state.lock().unwrap().is_none());
    }

    /// 空バッファでも停止時にFLACヘッダーが返る
    #[test]
    fn stop_recording_handles_empty_buffer() {
        init_env_config_for_test();
        // 空のバッファでの動作をテスト
        let backend = CpalAudioBackend::default();

        // テスト用の空のMemoryRecordingStateを設定
        let buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
        *backend.recording_state.lock().unwrap() = Some(MemoryRecordingState {
            buffer: buffer.clone(),
            sample_rate: 44100,
            channels: 2,
            generation: 1,
            accepting_input: Arc::new(AtomicBool::new(true)),
        });
        backend.capture_generation.store(1, Ordering::SeqCst);

        // 録音フラグを設定
        backend.recording.store(true, Ordering::SeqCst);

        // stop_recordingを実行
        let result = backend.stop_recording().unwrap();

        // 空のデータでもFLACヘッダーは生成される
        assert_eq!(result.mime_type, "audio/flac");
        assert!(result.bytes.len() > 4);
        assert_eq!(&result.bytes[0..4], b"fLaC");

        // 録音状態がクリアされていることを確認
        assert!(!backend.is_recording());
        assert!(backend.recording_state.lock().unwrap().is_none());
    }

    /// 30秒録音のメモリ使用量が想定範囲に収まる
    #[test]
    fn memory_usage_for_30s_recording() {
        // 30秒録音のメモリ使用量テスト
        let sample_rate = 48000u32;
        let channels = 2u16;
        let duration_secs = 30u32;

        // サンプル数を計算
        let total_samples = sample_rate * channels as u32 * duration_secs;

        // i16のバッファを作成（実際のメモリ使用量を確認）
        let buffer: Vec<i16> = vec![0; total_samples as usize];

        // メモリサイズの確認
        let memory_size_bytes = buffer.len() * std::mem::size_of::<i16>();
        let memory_size_mb = memory_size_bytes as f64 / (1024.0 * 1024.0);

        println!("30秒録音のメモリ使用量: {:.2} MB", memory_size_mb);

        // 期待値: 約5.5MB (48000 * 2 * 30 * 2bytes = 5,760,000 bytes ≈ 5.49 MB)
        assert!(memory_size_mb < 6.0, "メモリ使用量が6MBを超えています");
        assert!(memory_size_mb > 5.0, "メモリ使用量が予想より少なすぎます");

        // WAVデータ生成のテスト
        let wav_result = CpalAudioBackend::combine_wav_data(&buffer, sample_rate, channels);
        assert!(wav_result.is_ok());

        let wav_data = wav_result.unwrap();
        let wav_size_mb = wav_data.len() as f64 / (1024.0 * 1024.0);
        println!("WAVファイルサイズ: {:.2} MB", wav_size_mb);

        // WAVファイルサイズも同程度であることを確認（ヘッダー44バイト + データ）
        assert!((wav_size_mb - memory_size_mb).abs() < 0.01);
    }

    /// 見積もりサイズで事前確保できる
    #[test]
    fn buffer_capacity_matches_estimate() {
        // バッファの事前確保が適切に行われているかテスト
        let sample_rate = 48000;
        let channels = 2;
        let duration = 30;

        // estimate_buffer_sizeの結果を確認
        let estimated = CpalAudioBackend::estimate_buffer_size(duration, sample_rate, channels);
        let expected = sample_rate as usize * channels as usize * duration as usize;
        assert_eq!(estimated, expected);

        // Vec::with_capacityで作成した場合のキャパシティを確認
        let buffer: Vec<i16> = Vec::with_capacity(estimated);
        assert_eq!(buffer.capacity(), estimated);

        // 実際に要素を追加してもreallocが発生しないことを確認
        let mut buffer = buffer;
        buffer.resize(estimated, 0);
        // capacityが変わっていないことを確認（reallocが発生していない）
        assert_eq!(buffer.capacity(), estimated);
    }

    /// 実デバイスでメモリモード録音できる
    #[test]
    #[cfg_attr(feature = "ci-test", ignore)]
    fn real_device_records_in_memory_mode() {
        // 実際のデバイスでメモリモード録音をテスト（CI環境では無視）
        let backend = CpalAudioBackend::default();

        // 録音開始を試みる
        match backend.start_recording() {
            Ok(_) => {
                // 録音状態を確認
                assert!(backend.is_recording());

                // RecordingStateがMemoryモードであることを確認
                let state = backend.recording_state.lock().unwrap();
                match &*state {
                    Some(_) => {
                        println!("メモリモードで録音中");
                    }
                    None => panic!("Expected recording state"),
                }
                drop(state);

                // 少し待機（実際の録音をシミュレート）
                std::thread::sleep(std::time::Duration::from_millis(100));

                // 録音停止
                let result = backend.stop_recording().unwrap();
                let data = result.bytes;
                println!("録音データサイズ: {} bytes", data.len());
                assert!(data.len() > 4);
                assert_eq!(&data[0..4], b"fLaC");
            }
            Err(e) => {
                println!("録音開始失敗（デバイスなし）: {}", e);
            }
        }
    }

    /// 先頭と末尾の無音が除去される
    #[test]
    fn trim_silence_removes_leading_and_trailing_silence() {
        let sample_rate = 16_000;
        let channels = 1;
        let frame_size = channels as usize;

        let leading = vec![0i16; sample_rate as usize / 10 * frame_size];
        let signal = vec![2000i16; sample_rate as usize / 20 * frame_size];
        let trailing = vec![0i16; sample_rate as usize / 10 * frame_size];

        let mut samples = Vec::new();
        samples.extend_from_slice(&leading);
        samples.extend_from_slice(&signal);
        samples.extend_from_slice(&trailing);

        let trimmed = CpalAudioBackend::trim_silence(&samples, sample_rate, channels);

        assert_eq!(trimmed.len(), signal.len());
        assert!(trimmed.iter().all(|&s| s == 2000));
    }

    /// ステレオ音声でも無音除去が機能する
    #[test]
    fn trim_silence_handles_stereo_audio() {
        let sample_rate = 48_000;
        let channels = 2;
        let frame_size = channels as usize;

        let silent_samples = sample_rate as usize * frame_size * 15 / 100;
        let active_frames = sample_rate as usize / 100;
        let mut samples = Vec::with_capacity(silent_samples * 2 + active_frames * frame_size);
        samples.resize(silent_samples, 0);
        samples.extend((0..active_frames).flat_map(|_| [2500i16, -2500i16]));
        samples.resize(samples.len() + silent_samples, 0);

        let trimmed = CpalAudioBackend::trim_silence(&samples, sample_rate, channels);

        assert_eq!(trimmed.len(), sample_rate as usize / 100 * frame_size);
        assert!(
            trimmed
                .chunks(frame_size)
                .all(|frame| frame[0] == 2500 && frame[1] == -2500)
        );
    }

    /// 全て無音でも最低限のサンプルが残る
    #[test]
    fn trim_silence_keeps_minimum_when_all_silent() {
        let sample_rate = 16_000;
        let channels = 1;
        let samples = vec![0i16; sample_rate as usize / 10];

        let trimmed = CpalAudioBackend::trim_silence(&samples, sample_rate, channels);

        assert!(!trimmed.is_empty());
        assert!(trimmed.iter().all(|&s| s == 0));
    }

    /// 48kHz の音声を 16kHz に変換するとサンプル数が 1/3 になる
    #[test]
    fn resample_to_16khz_downscales_frame_count() {
        let sample_rate = 48_000;
        let samples = vec![1000i16; sample_rate as usize];

        let resampled = CpalAudioBackend::resample_to_16khz(&samples, sample_rate).unwrap();

        assert_eq!(resampled.samples.len(), 16_000);
        assert_eq!(resampled.sample_rate, TARGET_SAMPLE_RATE);
    }

    /// すでに 16kHz の場合はリサンプリングを行わない
    #[test]
    fn resample_to_16khz_skips_when_rate_matches() {
        let sample_rate = 16_000;
        let samples = vec![1000i16; sample_rate as usize];

        let resampled = CpalAudioBackend::resample_to_16khz(&samples, sample_rate).unwrap();

        assert_eq!(resampled.samples.len(), samples.len());
        assert_eq!(resampled.samples, samples);
        assert_eq!(resampled.sample_rate, sample_rate);
    }

    /// 極端に短い入力はリサンプリングをスキップする
    #[test]
    fn resample_to_16khz_skips_when_too_short() {
        let sample_rate = 48_000;
        let samples = vec![1000i16; MIN_RESAMPLE_FRAMES - 1];

        let resampled = CpalAudioBackend::resample_to_16khz(&samples, sample_rate).unwrap();

        assert_eq!(resampled.samples.len(), samples.len());
        assert_eq!(resampled.samples, samples);
        assert_eq!(resampled.sample_rate, sample_rate);
    }
}
