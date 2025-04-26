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

pub struct CpalAudioBackend {
    stream: Mutex<Option<Stream>>,
    recording: Arc<AtomicBool>,
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

/// `INPUT_DEVICE_PRIORITY` を解釈して使用デバイスを決定する
fn select_input_device(host: &cpal::Host) -> Option<Device> {
    use std::env;

    // 1) 環境変数から優先リストを取得
    let priorities: Vec<String> = env::var("INPUT_DEVICE_PRIORITY")
        .ok()?
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    // 2) システム上の入力デバイスを列挙
    let available: Vec<Device> = host.input_devices().ok()?.collect();

    // 3) 優先順位で探索
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

// =============== 既存実装 ================================

impl CpalAudioBackend {
    fn make_output_path() -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut p = std::env::temp_dir();
        p.push(format!("voice_input_{ts}.wav"));
        p.to_string_lossy().into_owned()
    }

    fn build_input_stream(
        recording: Arc<AtomicBool>,
        device: &Device,
        config: &StreamConfig,
        sample_format: SampleFormat,
        output_path: String,
    ) -> Result<Stream, Box<dyn Error>> {
        // WAV header
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
    fn start_recording(&self) -> Result<(), Box<dyn Error>> {
        if self.is_recording() {
            return Err("already recording".into());
        }

        // ─── Host とデバイス選択 ─────────────────────────
        let host = cpal::default_host();
        let device = select_input_device(&host)
            .ok_or("no input device available (check INPUT_DEVICE_PRIORITY)")?;

        let supported = device.default_input_config()?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        // ─── WAV 出力パス & ストリーム ────────────────────
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

    fn stop_recording(&self) -> Result<String, Box<dyn Error>> {
        if !self.is_recording() {
            return Err("not recording".into());
        }
        *self.stream.lock().unwrap() = None; // drop stream
        self.recording.store(false, Ordering::SeqCst);

        let path = self
            .output_path
            .lock()
            .unwrap()
            .take()
            .ok_or("output path not set")?;
        Ok(path)
    }

    fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }
}
