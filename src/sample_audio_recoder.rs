use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use hound;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn record_audio() {
    // CPALのデフォルトホストと入力デバイスを取得する
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .expect("入力デバイスが見つからんけぇ");
    println!("入力デバイス: {}", device.name().unwrap());

    // 入力設定を取得する
    let config = device
        .default_input_config()
        .expect("デフォルト入力設定が取得できんけぇ");
    println!("入力設定: {:?}", config);

    // WAVファイルの設定に必要なサンプルレートを保存
    let sample_rate = config.sample_rate().0;

    // ここが重要: 録音のたびに新しいバッファを作成する
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));

    // エラーコールバック
    let err_fn = |err| eprintln!("エラー発生: {:?}", err);

    // サンプルフォーマットに応じてストリームを構築する
    let stream = match config.sample_format() {
        SampleFormat::F32 => {
            build_stream::<f32>(&device, &config.config().clone(), samples.clone(), err_fn)
        }
        SampleFormat::I16 => {
            build_stream::<i16>(&device, &config.config().clone(), samples.clone(), err_fn)
        }
        SampleFormat::U16 => {
            build_stream::<u16>(&device, &config.config().clone(), samples.clone(), err_fn)
        }
        _ => panic!("サポートされていないサンプルフォーマットです"),
    };

    // ストリーム再生開始（録音開始）
    stream.play().expect("ストリームの再生に失敗しとる");

    println!("5秒間録音しとるけぇ……");
    thread::sleep(Duration::from_secs(5));

    // 重要: ストリームを明示的に停止して、データの追加を止める
    stream.pause().expect("ストリームの停止に失敗しとる");

    // 録音終了
    let recorded_samples = samples.lock().unwrap().clone();
    if recorded_samples.is_empty() {
        println!("録音サンプルが一つも取れてへんけぇ");
        return;
    }

    // WAVファイルの設定
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let filename = format!(
        "recording_{}.wav",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );
    let mut writer =
        hound::WavWriter::create(filename, spec).expect("WAVファイルの作成に失敗しとる");

    // サンプルをWAVファイルに書き出す
    for sample in recorded_samples.iter() {
        let clamped = sample.max(-1.0).min(1.0);
        let value = (clamped * i16::MAX as f32) as i16;
        writer
            .write_sample(value)
            .expect("サンプルの書き込みに失敗しとる");
    }
    writer.finalize().expect("WAVファイルの確定に失敗しとる");

    println!("WAVファイルとして 'recording.wav' に保存したけぇ");
}

// 指定したサンプルフォーマットで入力ストリームを構築する関数
fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> cpal::Stream
where
    T: Sample + cpal::SizedSample + Send + 'static,
    <T as Sample>::Float: std::convert::Into<f32>,
{
    // 重要な修正: ストリーム作成前にバッファをクリア
    {
        let mut samples_lock = samples.lock().unwrap();
        samples_lock.clear();
    }

    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                // 入力されたサンプルをf32に変換してバッファへ追加
                let mut samples_lock = samples.lock().unwrap();
                for &sample in data.iter() {
                    samples_lock.push(sample.to_float_sample().into());
                }
            },
            err_fn,
            None,
        )
        .expect("入力ストリームの構築に失敗しとる")
}
