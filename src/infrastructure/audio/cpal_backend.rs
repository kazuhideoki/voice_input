use chrono::Utc;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use hound;
use std::cell::RefCell;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{self, Sender};

/// ------------ ファイルパス定数 ------------
pub const RECORDING_STATUS_FILE: &str = "/tmp/voice_input_recording_status.txt";
const LAST_WAV_PATH_FILE: &str = "/tmp/voice_input_last_recording.txt";
const TEMP_WAV_PATH: &str = "/tmp/voice_input_recorded.wav";

/// ------------ グローバル録音状態 ------------
pub struct RecordingState {
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    is_recording: bool,
    stream: Option<cpal::Stream>,
    stop_signal: Option<Sender<()>>,
}

thread_local! {
    static REC_STATE: RefCell<RecordingState> = RefCell::new(RecordingState {
        samples:     Arc::new(Mutex::new(Vec::new())),
        sample_rate: 44100,
        is_recording:false,
        stream:      None,
        stop_signal: None,
    });
}

/// ==========================================
/// 録音開始（時間指定可 / None なら無制限）
/// ==========================================
pub async fn start_recording(
    max_secs: Option<u64>,
    timeout_tx: Sender<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    if Path::new(RECORDING_STATUS_FILE).exists() {
        return Err("すでに録音中です".into());
    }

    REC_STATE.with(|cell| {
        let mut st = cell.borrow_mut();

        init_input_stream(&mut st)?;

        if let Some(stream) = &st.stream {
            stream.play()?;
            st.is_recording = true;

            fs::write(RECORDING_STATUS_FILE, "recording")?;
            fs::write(LAST_WAV_PATH_FILE, TEMP_WAV_PATH)?;

            // 録音自動停止タイマー
            if let Some(sec) = max_secs {
                let (tx, mut rx) = mpsc::channel::<()>(1);
                st.stop_signal = Some(tx);
                tokio::spawn(async move {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(sec)) => {
                            let _ = timeout_tx.send(()).await;
                        }
                        _ = rx.recv() => {}
                    }
                });
            }
            Ok(())
        } else {
            Err("入力ストリームの初期化に失敗".into())
        }
    })
}

/// ==========================================
/// 録音停止 & WAV 保存（パスを返す）
/// ==========================================
pub async fn stop_recording() -> Result<String, Box<dyn std::error::Error>> {
    if Path::new(RECORDING_STATUS_FILE).exists() {
        fs::remove_file(RECORDING_STATUS_FILE).ok();
    }

    let (wav_path, maybe_tx) = REC_STATE.with(|cell| {
        let mut st = cell.borrow_mut();

        // …同一プロセス外からの呼び出し保険は省略（元実装のまま）…

        if let Some(stream) = &st.stream {
            stream.pause()?;
        }
        let tx = st.stop_signal.take();
        st.is_recording = false;
        let path = save_wav(&st)?;
        fs::write(LAST_WAV_PATH_FILE, &path)?;
        Ok::<_, Box<dyn std::error::Error>>((path, tx))
    })?;

    if let Some(tx) = maybe_tx {
        let _ = tx.send(()).await;
    }
    Ok(wav_path)
}

/// ------------ 内部ユーティリティ ------------

fn init_input_stream(st: &mut RecordingState) -> Result<(), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = pick_input_device(&host).ok_or("入力デバイスが無い")?;
    let cfg = device.default_input_config()?;

    st.sample_rate = cfg.sample_rate().0;
    st.samples = Arc::new(Mutex::new(Vec::new()));

    let err_fn = |e| eprintln!("CPAL error: {:?}", e);
    let stream = match cfg.sample_format() {
        SampleFormat::F32 => {
            build_stream::<f32>(&device, &cfg.config(), st.samples.clone(), err_fn)
        }
        SampleFormat::I16 => {
            build_stream::<i16>(&device, &cfg.config(), st.samples.clone(), err_fn)
        }
        SampleFormat::U16 => {
            build_stream::<u16>(&device, &cfg.config(), st.samples.clone(), err_fn)
        }
        _ => return Err("未サポートのフォーマット".into()),
    };
    st.stream = Some(stream);
    Ok(())
}

fn pick_input_device(host: &cpal::Host) -> Option<cpal::Device> {
    // 環境変数からデバイス優先順位を取得
    let priority_devices = match env::var("INPUT_DEVICE_PRIORITY") {
        Ok(value) => {
            // 環境変数が設定されている場合、カンマで分割
            let devices: Vec<String> = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if devices.is_empty() {
                println!(
                    "環境変数 INPUT_DEVICE_PRIORITY が空です。デフォルトの優先順位を使用します。"
                );
                panic!("デバイス優先順位が空です。")
            } else {
                println!("環境変数から読み込んだ優先デバイスリスト: {:?}", devices);
                devices
            }
        }
        Err(_) => {
            println!(
                "環境変数 INPUT_DEVICE_PRIORITY が設定されていません。デフォルトの優先順位を使用します。"
            );
            panic!("デバイス優先順位が設定されていません。")
        }
    };

    // 利用可能なすべての入力デバイスを取得
    let input_devices = match host.input_devices() {
        Ok(devices) => devices.collect::<Vec<_>>(),
        Err(e) => {
            println!("入力デバイスの一覧取得に失敗しました: {:?}", e);
            return host.default_input_device();
        }
    };

    println!("利用可能な入力デバイス:");
    for (i, device) in input_devices.iter().enumerate() {
        if let Ok(name) = device.name() {
            println!("  {}. {}", i + 1, name);
        }
    }

    // 優先順位に従ってデバイスを探す
    for device_name in priority_devices.iter() {
        for device in &input_devices {
            if let Ok(name) = device.name() {
                if name == device_name.as_str() {
                    println!("優先デバイスを選択: {}", name);
                    return Some(device.clone());
                }
            }
        }
    }

    // ---------- ③ フォールバック ----------
    println!("優先リストが無い / 見つからないのでデフォルトを使用");
    host.default_input_device()
}

fn save_wav(st: &RecordingState) -> Result<String, Box<dyn std::error::Error>> {
    let data = st.samples.lock().unwrap();
    if data.is_empty() {
        return Err("録音データが空".into());
    }

    let file = format!(
        "/tmp/voice_input_{}.wav",
        Utc::now().format("%Y%m%d_%H%M%S")
    );
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: st.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&file, spec)?;
    for &s in data.iter() {
        let v = (s.max(-1.0).min(1.0) * i16::MAX as f32) as i16;
        writer.write_sample(v)?;
    }
    writer.finalize()?;
    Ok(file)
}

fn build_stream<T>(
    device: &cpal::Device,
    cfg: &cpal::StreamConfig,
    buf: Arc<Mutex<Vec<f32>>>,
    mut err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> cpal::Stream
where
    T: Sample + cpal::SizedSample + Send + 'static,
    <T as Sample>::Float: Into<f32>,
{
    buf.lock().unwrap().clear();
    device
        .build_input_stream(
            cfg,
            move |data: &[T], _| {
                let mut v = buf.lock().unwrap();
                for &s in data {
                    v.push(s.to_float_sample().into());
                }
            },
            move |e| err_fn(e),
            None,
        )
        .expect("input stream build failed")
}
