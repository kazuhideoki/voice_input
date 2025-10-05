use super::AudioBackend;
use crate::utils::config::EnvConfig;
use super::encoder::{self, AudioFormat};
use cpal::{
    Device, SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    borrow::Cow,
    error::Error,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
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
}

/// Audio processing errors
#[derive(Debug)]
pub enum AudioError {
    DataTooLarge(usize),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::DataTooLarge(size) => {
                write!(f, "PCM data too large: {} bytes exceeds u32 max", size)
            }
        }
    }
}

impl Error for AudioError {}

/// CpalAudioBackend 向けのエラー型（public APIの意味が伝わるメッセージ）
#[derive(Debug)]
pub enum CpalBackendError {
    AlreadyRecording,
    NoInputDevice,
    UnsupportedSampleFormat,
    NotRecording,
    RecordingStateNotSet,
}

impl fmt::Display for CpalBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CpalBackendError::AlreadyRecording => write!(f, "recording is already in progress"),
            CpalBackendError::NoInputDevice => {
                write!(f, "no input device available (check INPUT_DEVICE_PRIORITY)")
            }
            CpalBackendError::UnsupportedSampleFormat => write!(f, "unsupported sample format"),
            CpalBackendError::NotRecording => write!(f, "not currently recording"),
            CpalBackendError::RecordingStateNotSet => write!(f, "recording state not set"),
        }
    }
}

impl Error for CpalBackendError {}

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
    stream: Mutex<Option<Stream>>,
    /// 録音フラグ
    recording: Arc<AtomicBool>,
    /// 録音状態（メモリモード専用）
    recording_state: Mutex<Option<MemoryRecordingState>>,
}

impl Default for CpalAudioBackend {
    fn default() -> Self {
        Self {
            stream: Mutex::new(None),
            recording: Arc::new(AtomicBool::new(false)),
            recording_state: Mutex::new(None),
        }
    }
}

/// `INPUT_DEVICE_PRIORITY` 環境変数を解釈し、優先順位の高い入力デバイスを選択します。
fn select_input_device(host: &cpal::Host) -> Option<Device> {
    use std::env;

    // 1) 優先リスト取得 (カンマ区切り)
    let priorities: Vec<String> = env::var("INPUT_DEVICE_PRIORITY")
        .ok()?
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    // 2) 利用可能なデバイスを列挙
    let available: Vec<Device> = host.input_devices().ok()?.collect();

    // 3) 優先度順に一致デバイスを探す
    for want in &priorities {
        if let Some(dev) = available
            .iter()
            .find(|d| d.name().map(|n| n == *want).unwrap_or(false))
        {
            println!("🎙️  Using preferred device: {}", want);
            return Some(dev.clone());
        }
    }

    // 4) 見つからなければデフォルト
    println!("⚠️  No preferred device found, falling back to default input device");
    host.default_input_device()
}

// =============== WAVヘッダー生成機能 ================================
impl CpalAudioBackend {
    fn preferred_format() -> AudioFormat {
        let cfg = EnvConfig::get();
        match std::env::var("VOICE_INPUT_AUDIO_FORMAT").ok()
            .or_else(|| cfg.openai_api_key.as_ref().map(|_| "flac".to_string()))
            .unwrap_or_else(|| "flac".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "wav" => AudioFormat::Wav,
            _ => AudioFormat::Flac,
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

    /// 利用可能な入力デバイス名を返すユーティリティ
    pub fn list_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|iter| iter.filter_map(|d| d.name().ok()).collect::<Vec<String>>())
            .unwrap_or_default()
    }

    /// メモリモード用のストリーム構築
    fn build_memory_stream(
        recording: Arc<AtomicBool>,
        device: &Device,
        config: &StreamConfig,
        sample_format: SampleFormat,
        buffer: Arc<Mutex<Vec<i16>>>,
    ) -> Result<Stream, Box<dyn Error>> {
        let stream = match sample_format {
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _| {
                    if recording.load(Ordering::SeqCst) {
                        let mut buf = buffer.lock().unwrap();
                        buf.extend_from_slice(data);
                    }
                },
                |e| eprintln!("stream error: {e}"),
                None,
            )?,
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _| {
                    if recording.load(Ordering::SeqCst) {
                        let mut buf = buffer.lock().unwrap();
                        buf.extend(
                            data.iter()
                                .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16),
                        );
                    }
                },
                |e| eprintln!("stream error: {e}"),
                None,
            )?,
            _ => return Err(CpalBackendError::UnsupportedSampleFormat.into()),
        };

        Ok(stream)
    }
}

impl AudioBackend for CpalAudioBackend {
    /// 録音ストリームを開始します。
    fn start_recording(&self) -> Result<(), Box<dyn Error>> {
        if self.is_recording() {
            return Err(CpalBackendError::AlreadyRecording.into());
        }

        // ホスト・デバイス取得
        let host = cpal::default_host();
        let device = select_input_device(&host).ok_or(CpalBackendError::NoInputDevice)?;

        let supported = device.default_input_config()?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        // メモリモード: バッファベース
        let sample_rate = config.sample_rate.0;
        let channels = config.channels;

        // 30秒分のバッファを事前確保
        let capacity = Self::estimate_buffer_size(30, sample_rate, channels);
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(capacity)));

        // RecordingStateをMemモリモードに設定
        *self.recording_state.lock().unwrap() = Some(MemoryRecordingState {
            buffer: buffer.clone(),
            sample_rate,
            channels,
        });

        let stream = Self::build_memory_stream(
            self.recording.clone(),
            &device,
            &config,
            sample_format,
            buffer,
        )?;

        stream.play()?;
        self.recording.store(true, Ordering::SeqCst);
        *self.stream.lock().unwrap() = Some(stream);
        Ok(())
    }

    /// 録音を停止し、音声データを返します。
    fn stop_recording(&self) -> Result<AudioData, Box<dyn Error>> {
        if !self.is_recording() {
            return Err(CpalBackendError::NotRecording.into());
        }

        // ストリームを解放して終了
        *self.stream.lock().unwrap() = None;
        self.recording.store(false, Ordering::SeqCst);

        // RecordingStateを取得
        let state = self
            .recording_state
            .lock()
            .unwrap()
            .take()
            .ok_or(CpalBackendError::RecordingStateNotSet)?;

        // メモリモード: バッファからエンコード（既定: FLAC）
        let samples = state.buffer.lock().unwrap();
        let trimmed = Self::trim_silence(&samples, state.sample_rate, state.channels);
        match Self::preferred_format() {
            AudioFormat::Flac => {
                match encoder::flac::encode_flac_i16(&trimmed, state.sample_rate, state.channels) {
                    Ok(flac) => Ok(AudioData {
                        bytes: flac,
                        mime_type: "audio/flac",
                        file_name: "audio.flac".to_string(),
                    }),
                    Err(e) => {
                        eprintln!("FLAC encode failed (fallback to WAV): {}", e);
                        let wav = Self::combine_wav_data(&trimmed, state.sample_rate, state.channels)?;
                        Ok(AudioData { bytes: wav, mime_type: "audio/wav", file_name: "audio.wav".to_string() })
                    }
                }
            }
            AudioFormat::Wav => {
                let wav = Self::combine_wav_data(&trimmed, state.sample_rate, state.channels)?;
                Ok(AudioData { bytes: wav, mime_type: "audio/wav", file_name: "audio.wav".to_string() })
            }
        }
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

    /// `INPUT_DEVICE_PRIORITY` に存在しないデバイスを設定し、バックエンドが
    /// (1) フォールバックを介して開始する **または** (2) 入力デバイスの欠落に
    /// 言及するエラーを返すことを確認します。これにより、優先順位/フォールバック
    /// コードが誤って削除されることを防ぎます。
    use std::sync::Mutex;

    static INPUT_DEVICE_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn input_device_priority_env_is_handled() {
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

    #[test]
    fn test_wav_header_structure() {
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

    #[test]
    fn test_wav_header_mono() {
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

    #[test]
    fn test_wav_header_various_sample_rates() {
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

    #[test]
    fn test_wav_header_empty_data() {
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

    #[test]
    fn test_sample_trait_i16() {
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

    #[test]
    fn test_sample_trait_f32() {
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

    #[test]
    fn test_sample_f32_to_le_bytes() {
        // f32 -> bytes 変換テスト
        let sample: f32 = 0.0;
        assert_eq!(Sample::as_pcm_le_bytes(&sample), [0x00, 0x00]);

        let sample: f32 = 1.0;
        let bytes = Sample::as_pcm_le_bytes(&sample);
        let reconstructed = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(reconstructed, i16::MAX);
    }

    #[test]
    fn test_combine_wav_data_i16() {
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

    #[test]
    fn test_combine_wav_data_f32() {
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

    #[test]
    fn test_combine_wav_data_empty() {
        // 空のPCMデータ
        let pcm_data: Vec<i16> = vec![];
        let result = CpalAudioBackend::combine_wav_data(&pcm_data, 48000, 2).unwrap();

        // ヘッダーのみ
        assert_eq!(result.len(), 44);

        // データサイズは0
        let data_size = u32::from_le_bytes([result[40], result[41], result[42], result[43]]);
        assert_eq!(data_size, 0);
    }

    #[test]
    fn test_combine_wav_data_stereo_interleaved() {
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

    #[test]
    fn test_memory_mode_only() {
        // メモリモード専用になったことを確認
        let backend = CpalAudioBackend::default();

        // 録音状態は初期状態でNone
        assert!(backend.recording_state.lock().unwrap().is_none());

        // 録音中でない
        assert!(!backend.is_recording());
    }

    #[test]
    fn test_audio_data_struct() {
        // Data creation
        let data = vec![1, 2, 3, 4, 5];
        let audio_data = AudioData { bytes: data.clone(), mime_type: "audio/wav", file_name: "audio.wav".to_string() };

        // Data access
        assert_eq!(audio_data.bytes, data);

        // Debug trait
        assert!(format!("{:?}", audio_data).contains("AudioData"));

        // Clone trait
        let cloned = audio_data.clone();
        assert_eq!(cloned.bytes, data);
    }

    #[test]
    fn test_memory_recording_state_creation() {
        // メモリモードの状態作成
        let buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
        let memory_state = MemoryRecordingState {
            buffer: buffer.clone(),
            sample_rate: 48000,
            channels: 2,
        };

        // bufferが適切に初期化されているか確認
        assert_eq!(memory_state.sample_rate, 48000);
        assert_eq!(memory_state.channels, 2);
        assert!(memory_state.buffer.lock().unwrap().is_empty());
    }

    #[test]
    fn test_cpal_backend_with_recording_state() {
        let backend = CpalAudioBackend::default();

        // 初期状態はNone
        assert!(backend.recording_state.lock().unwrap().is_none());

        // recording_stateフィールドが存在することを確認
        assert!(!backend.is_recording());
    }

    #[test]
    fn test_estimate_buffer_size() {
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

    #[test]
    fn test_start_recording_setup() {
        let backend = CpalAudioBackend::default();

        // 録音開始前の状態確認
        assert!(!backend.is_recording());
        assert!(backend.recording_state.lock().unwrap().is_none());

        // 注意: 実際のデバイスが必要なため、CI環境では失敗する可能性がある
        // ここではバックエンドの初期状態のみをテストする
    }

    #[test]
    fn test_backend_initialization() {
        let backend = CpalAudioBackend::default();

        // 録音開始前の状態確認
        assert!(!backend.is_recording());
        assert!(backend.recording_state.lock().unwrap().is_none());

        // streamも初期状態でNone
        assert!(backend.stream.lock().unwrap().is_none());
    }

    #[test]
    fn test_stop_recording_memory_mode() {
        // メモリモードでの動作をシミュレート
        let backend = CpalAudioBackend::default();

        // テスト用のMemoryRecordingStateを設定
        let buffer = Arc::new(Mutex::new(vec![100i16, -100, 0, 1000, -1000]));
        *backend.recording_state.lock().unwrap() = Some(MemoryRecordingState {
            buffer: buffer.clone(),
            sample_rate: 48000,
            channels: 1,
        });

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

    #[test]
    fn test_stop_recording_with_empty_buffer() {
        // 空のバッファでの動作をテスト
        let backend = CpalAudioBackend::default();

        // テスト用の空のMemoryRecordingStateを設定
        let buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
        *backend.recording_state.lock().unwrap() = Some(MemoryRecordingState {
            buffer: buffer.clone(),
            sample_rate: 44100,
            channels: 2,
        });

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

    #[test]
    fn test_memory_usage_30_seconds() {
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

    #[test]
    fn test_buffer_capacity_optimization() {
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

    #[test]
    #[cfg_attr(feature = "ci-test", ignore)]
    fn test_real_recording_memory_mode() {
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

    #[test]
    fn trim_silence_keeps_minimum_when_all_silent() {
        let sample_rate = 16_000;
        let channels = 1;
        let samples = vec![0i16; sample_rate as usize / 10];

        let trimmed = CpalAudioBackend::trim_silence(&samples, sample_rate, channels);

        assert!(!trimmed.is_empty());
        assert!(trimmed.iter().all(|&s| s == 0));
    }
}
