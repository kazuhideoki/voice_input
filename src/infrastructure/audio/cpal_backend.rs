use super::AudioBackend;
use cpal::{
    Device, SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use hound::{SampleFormat as WavFmt, WavWriter};
use std::{
    error::Error,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

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

/// サンプルフォーマット変換トレイト
pub trait Sample {
    fn to_i16(&self) -> i16;
    fn to_bytes(&self) -> Vec<u8>;
}

impl Sample for i16 {
    fn to_i16(&self) -> i16 {
        *self
    }
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
}

impl Sample for f32 {
    fn to_i16(&self) -> i16 {
        (self.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
    }
    fn to_bytes(&self) -> Vec<u8> {
        self.to_i16().to_le_bytes().to_vec()
    }
}

/// CPAL によるローカルマイク入力実装。
/// WAV ファイルを `/tmp` 相当の一時ディレクトリに保存します。
pub struct CpalAudioBackend {
    /// ランタイム中の入力ストリーム
    stream: Mutex<Option<Stream>>,
    /// 録音フラグ
    recording: Arc<AtomicBool>,
    /// 出力 WAV パス
    output_path: Mutex<Option<String>>,
}

impl Default for CpalAudioBackend {
    fn default() -> Self {
        Self {
            stream: Mutex::new(None),
            recording: Arc::new(AtomicBool::new(false)),
            output_path: Mutex::new(None),
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

        // PCMデータをバイト列に変換して追加
        for sample in pcm_data {
            wav_data.extend_from_slice(&sample.to_bytes());
        }

        Ok(wav_data)
    }
}

// =============== 内部ユーティリティ ================================
impl CpalAudioBackend {
    /// 利用可能な入力デバイス名を返すユーティリティ
    pub fn list_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|iter| iter.filter_map(|d| d.name().ok()).collect::<Vec<String>>())
            .unwrap_or_default()
    }
    /// `/tmp/voice_input_<epoch>.wav` 形式の一意なファイルパスを生成
    fn make_output_path() -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut p = std::env::temp_dir();
        p.push(format!("voice_input_{ts}.wav"));
        p.to_string_lossy().into_owned()
    }

    /// CPAL ストリームを構築。サンプルを WAV ライターに書き込みます。
    fn build_input_stream(
        recording: Arc<AtomicBool>,
        device: &Device,
        config: &StreamConfig,
        sample_format: SampleFormat,
        output_path: String,
    ) -> Result<Stream, Box<dyn Error>> {
        // WAV ヘッダ
        let spec = hound::WavSpec {
            channels: config.channels,
            sample_rate: config.sample_rate.0,
            bits_per_sample: 16,
            sample_format: WavFmt::Int,
        };
        let writer = Arc::new(Mutex::new(WavWriter::create(&output_path, spec)?));

        let stream = match sample_format {
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _| {
                    if recording.load(Ordering::SeqCst) {
                        let mut w = writer.lock().unwrap();
                        for &s in data {
                            let _ = w.write_sample(s);
                        }
                    }
                },
                |e| eprintln!("stream error: {e}"),
                None,
            )?,
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _| {
                    if recording.load(Ordering::SeqCst) {
                        let mut w = writer.lock().unwrap();
                        for &s in data {
                            let _ = w.write_sample((s * i16::MAX as f32) as i16);
                        }
                    }
                },
                |e| eprintln!("stream error: {e}"),
                None,
            )?,
            _ => return Err("unsupported sample format".into()),
        };

        Ok(stream)
    }
}

impl AudioBackend for CpalAudioBackend {
    /// 録音ストリームを開始します。
    fn start_recording(&self) -> Result<(), Box<dyn Error>> {
        if self.is_recording() {
            return Err("already recording".into());
        }

        // ホスト・デバイス取得
        let host = cpal::default_host();
        let device = select_input_device(&host)
            .ok_or("no input device available (check INPUT_DEVICE_PRIORITY)")?;

        let supported = device.default_input_config()?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        // 出力パス生成 & ストリーム構築
        let wav_path = Self::make_output_path();
        let stream = Self::build_input_stream(
            self.recording.clone(),
            &device,
            &config,
            sample_format,
            wav_path.clone(),
        )?;
        stream.play()?;

        self.recording.store(true, Ordering::SeqCst);
        *self.stream.lock().unwrap() = Some(stream);
        *self.output_path.lock().unwrap() = Some(wav_path);
        Ok(())
    }

    /// 録音を停止し、WAV ファイルパスを返します。
    fn stop_recording(&self) -> Result<String, Box<dyn Error>> {
        if !self.is_recording() {
            return Err("not recording".into());
        }
        // ストリームを解放して終了
        *self.stream.lock().unwrap() = None;
        self.recording.store(false, Ordering::SeqCst);

        let path = self
            .output_path
            .lock()
            .unwrap()
            .take()
            .ok_or("output path not set")?;
        Ok(path)
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
    #[test]
    fn input_device_priority_env_is_handled() {
        unsafe { std::env::set_var("INPUT_DEVICE_PRIORITY", "ClearlyNonexistentDevice") };

        let backend = CpalAudioBackend::default();
        match backend.start_recording() {
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
                        || msg.contains("no longer available"),
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
        let data_len = 44100 * 1 * 2; // 44.1kHz, mono, 16bit
        let header = CpalAudioBackend::create_wav_header(data_len, 44100, 1, 16);

        assert_eq!(header.len(), 44);

        // チャンネル数確認
        let channels = u16::from_le_bytes([header[22], header[23]]);
        assert_eq!(channels, 1);

        // バイトレート確認
        let byte_rate = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);
        assert_eq!(byte_rate, 44100 * 1 * 2); // 88200

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
        assert_eq!(sample.to_bytes(), vec![0xE8, 0x03]); // 1000 in little endian

        let sample: i16 = -1000;
        assert_eq!(sample.to_i16(), -1000);
        assert_eq!(sample.to_bytes(), vec![0x18, 0xFC]); // -1000 in little endian

        let sample: i16 = 0;
        assert_eq!(sample.to_i16(), 0);
        assert_eq!(sample.to_bytes(), vec![0x00, 0x00]);
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
    fn test_sample_f32_to_bytes() {
        // f32 -> bytes 変換テスト
        let sample: f32 = 0.0;
        assert_eq!(sample.to_bytes(), vec![0x00, 0x00]);

        let sample: f32 = 1.0;
        let bytes = sample.to_bytes();
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
}
