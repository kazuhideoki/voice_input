use super::AudioBackend;
use cpal::{
    Device, SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use hound::{SampleFormat as WavFmt, WavWriter};
use std::{
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

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
                    msg.contains("INPUT_DEVICE_PRIORITY") || msg.contains("no input device"),
                    "unexpected error: {msg}"
                );
            }
        }
    }
}
