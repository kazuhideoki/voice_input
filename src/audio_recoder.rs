use crate::transcribe_audio;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use hound;
use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

// グローバル状態を管理するための構造体
pub struct RecordingState {
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    is_recording: bool,
    stream: Option<cpal::Stream>,
    stop_signal: Option<mpsc::Sender<()>>,
}

// スレッドローカルストレージで状態を保持
thread_local! {
    static RECORDING_STATE: RefCell<RecordingState> = RefCell::new(RecordingState {
        samples: Arc::new(Mutex::new(Vec::new())),
        sample_rate: 44100,
        is_recording: false,
        stream: None,
        stop_signal: None,
    });
}

// 録音状態を管理するためのファイルパス
const RECORDING_STATUS_FILE: &str = "recording_status.txt";
const LAST_RECORDING_FILE: &str = "last_recording.txt";

pub fn is_recording() -> bool {
    Path::new(RECORDING_STATUS_FILE).exists()
}

// 録音を開始する関数（時間指定可能、Noneなら無限に録音）
pub async fn record_with_duration(
    duration_secs: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 録音状態を確認
    if is_recording() {
        return Err("すでに録音中です".into());
    }

    // スレッドローカル状態を取得
    let result = RECORDING_STATE.with(|state_cell| {
        let mut state = state_cell.borrow_mut();

        // 録音準備
        prepare_recording(&mut state);

        // 録音開始
        if let Some(stream) = &state.stream {
            stream.play().expect("ストリームの再生に失敗しとる");
            state.is_recording = true;

            // 録音状態をファイルに保存（プロセス間で共有）
            fs::write(RECORDING_STATUS_FILE, "recording").expect("録音状態の保存に失敗しました");

            // 録音ファイル名をあらかじめ生成して保存
            let filename = format!(
                "recording_{}.wav",
                chrono::Local::now().format("%Y%m%d_%H%M%S")
            );
            println!("filenameは。。。 :{:?}", filename);
            fs::write(LAST_RECORDING_FILE, &filename).expect("録音ファイル名の保存に失敗しました");

            // 時間が指定されている場合は、停止シグナル用のチャネルを作成
            if let Some(duration) = duration_secs {
                let (tx, mut rx) = mpsc::channel::<()>(1);
                state.stop_signal = Some(tx);

                // 別スレッドで指定時間後に録音を停止するための通知チャネル
                let (notify_tx, mut notify_rx) = mpsc::channel::<()>(1);

                // 別スレッドで指定時間後に録音を停止
                tokio::spawn(async move {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(duration)) => {
                            // 時間が経過したので録音を停止
                            let _ = notify_tx.send(()).await;
                        }
                        _ = rx.recv() => {
                            // 明示的に停止された場合は何もしない
                        }
                    }
                });

                // メインスレッドに停止通知が来た場合の処理
                tokio::spawn(async move {
                    if notify_rx.recv().await.is_some() {
                        println!("タイムアウトにより録音を停止します...");
                        // ここでstop_recordingをモジュール外から呼び出す
                        std::process::Command::new("cargo")
                            .args(&["run", "--", "stop"])
                            .spawn()
                            .ok();
                    }
                });
            }

            Ok(())
        } else {
            Err("録音ストリームの初期化に失敗しました".into())
        }
    });

    result
}

// 録音を停止し、文字起こしを行う関数
pub async fn stop_recording() -> Result<String, Box<dyn std::error::Error>> {
    // 録音状態の確認（ファイルベースでの確認は同一プロセス内では不要）

    // プロセス外から呼び出された場合のためにファイルが存在していれば削除
    if is_recording() {
        fs::remove_file(RECORDING_STATUS_FILE).ok();
    }

    // 録音状態をチェックし、停止処理
    let (filename, tx_opt) = RECORDING_STATE.with(|state_cell| {
        let mut state = state_cell.borrow_mut();

        // ストリームが存在しない場合でも、同一プロセス内であれば録音状態をチェック
        if !state.is_recording && state.stream.is_none() && !is_recording() {
            // ストリームが無い場合でも、録音状態ファイルが存在しない場合はWAVファイルを探す
            let wav_files: Vec<_> = std::fs::read_dir(".")
                .expect("ディレクトリの読み取りに失敗しました")
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.path().extension().map_or(false, |ext| ext == "wav")
                        && entry.path().file_name().map_or(false, |name| {
                            name.to_string_lossy().starts_with("recording_")
                        })
                })
                .collect();

            if !wav_files.is_empty() {
                // 最新のWAVファイルを取得
                let latest_wav = wav_files
                    .into_iter()
                    .max_by_key(|entry| entry.metadata().unwrap().modified().unwrap())
                    .unwrap();
                let filename = latest_wav.file_name().to_string_lossy().to_string();

                // 最後の録音ファイル名を保存
                fs::write(LAST_RECORDING_FILE, &filename)
                    .expect("最後の録音ファイル名の保存に失敗しました");

                return Ok((filename, None));
            } else {
                return Err::<_, Box<dyn std::error::Error>>(
                    "録音ストリームが見つからず、過去の録音も見つかりません".into(),
                );
            }
        }

        // ストリームを停止（存在する場合）
        if let Some(stream) = &state.stream {
            stream.pause().expect("ストリームの停止に失敗しとる");
        }

        // 停止シグナルを取り出す
        let tx_opt = state.stop_signal.take();
        state.is_recording = false;

        // 録音データをファイルに保存
        let filename = save_recording_to_file(&state);

        // 最後の録音ファイル名を保存
        fs::write(LAST_RECORDING_FILE, &filename)
            .expect("最後の録音ファイル名の保存に失敗しました");

        Ok((filename, tx_opt))
    })?; // エラーの場合は早期リターン

    // 停止シグナル送信（ミューテックスガードの外で非同期処理）
    if let Some(tx) = tx_opt {
        let _ = tx.send(()).await;
    }

    // 文字起こし
    println!("音声をテキストに変換しとるけぇ... {:?}", filename);

    let transcription = transcribe_audio::transcribe_audio(&filename).await?;

    println!("文字起こし結果: {}", transcription);
    Ok(transcription)
}

// 録音の準備を行う関数
fn prepare_recording(state: &mut RecordingState) {
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
    state.sample_rate = config.sample_rate().0;

    // サンプルバッファをクリア
    state.samples = Arc::new(Mutex::new(Vec::<f32>::new()));

    // エラーコールバック
    let err_fn = |err| eprintln!("エラー発生: {:?}", err);

    // サンプルフォーマットに応じてストリームを構築する
    let stream = match config.sample_format() {
        SampleFormat::F32 => build_stream::<f32>(
            &device,
            &config.config().clone(),
            state.samples.clone(),
            err_fn,
        ),
        SampleFormat::I16 => build_stream::<i16>(
            &device,
            &config.config().clone(),
            state.samples.clone(),
            err_fn,
        ),
        SampleFormat::U16 => build_stream::<u16>(
            &device,
            &config.config().clone(),
            state.samples.clone(),
            err_fn,
        ),
        _ => panic!("サポートされていないサンプルフォーマットです"),
    };

    // ストリームを状態に保存
    state.stream = Some(stream);
}

// 録音データをファイルに保存する関数
fn save_recording_to_file(state: &RecordingState) -> String {
    // 録音データを取得
    let recorded_samples = state.samples.lock().unwrap().clone();
    if recorded_samples.is_empty() {
        println!("録音サンプルが一つも取れてへんけぇ");
        return String::new();
    }

    // WAVファイルの設定
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: state.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    // 新しいファイル名を生成
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("recording_{}.wav", timestamp);

    let mut writer =
        hound::WavWriter::create(&filename, spec).expect("WAVファイルの作成に失敗しとる");

    // サンプルをWAVファイルに書き出す
    for sample in recorded_samples.iter() {
        let clamped = sample.max(-1.0).min(1.0);
        let value = (clamped * i16::MAX as f32) as i16;
        writer
            .write_sample(value)
            .expect("サンプルの書き込みに失敗しとる");
    }
    writer.finalize().expect("WAVファイルの確定に失敗しとる");

    println!("WAVファイルとして '{}' に保存したけぇ", filename);
    filename
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
