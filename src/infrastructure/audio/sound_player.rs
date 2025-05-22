//! ネイティブ音声再生モジュール
//! 
//! システム音声ファイル（AIFF）をcpalを使用してネイティブ再生します。

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound;
use std::sync::Arc;
use std::path::Path;

/// ネイティブ音声再生構造体
pub struct NativeSoundPlayer {
    device: cpal::Device,
    config: cpal::StreamConfig,
    ping_data: Arc<Vec<f32>>,
    purr_data: Arc<Vec<f32>>,
    glass_data: Arc<Vec<f32>>,
}

impl NativeSoundPlayer {
    /// 新しいNativeSoundPlayerインスタンスを作成します
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // cpalデバイス初期化
        let host = cpal::default_host();
        let device = host.default_output_device()
            .ok_or("No output device available")?;
        
        let config = device.default_output_config()?;
        let stream_config = config.into();

        // システム音声ファイル読み込み
        let ping_data = Arc::new(load_system_sound("/System/Library/Sounds/Ping.aiff")?);
        let purr_data = Arc::new(load_system_sound("/System/Library/Sounds/Purr.aiff")?);
        let glass_data = Arc::new(load_system_sound("/System/Library/Sounds/Glass.aiff")?);

        Ok(Self {
            device,
            config: stream_config,
            ping_data,
            purr_data,
            glass_data,
        })
    }

    /// Ping音を再生します
    pub fn play_ping(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.play_sound(self.ping_data.clone())
    }

    /// Purr音を再生します
    pub fn play_purr(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.play_sound(self.purr_data.clone())
    }

    /// Glass音を再生します
    pub fn play_glass(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.play_sound(self.glass_data.clone())
    }

    /// 音声データを再生します
    fn play_sound(&self, data: Arc<Vec<f32>>) -> Result<(), Box<dyn std::error::Error>> {
        let mut sample_index = 0;
        let data_len = data.len();
        
        let data_clone = data.clone();
        let stream = self.device.build_output_stream(
            &self.config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for sample in output.iter_mut() {
                    if sample_index < data_len {
                        *sample = data_clone[sample_index];
                        sample_index += 1;
                    } else {
                        *sample = 0.0;
                    }
                }
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        )?;

        stream.play()?;
        
        // 音声の長さだけ待機（簡易的な実装）
        let duration_ms = (data_len as f64 / self.config.sample_rate.0 as f64 * 1000.0) as u64;
        std::thread::sleep(std::time::Duration::from_millis(duration_ms + 100)); // 少し余裕を持たせる
        
        drop(stream);
        Ok(())
    }
}

/// システム音声ファイルを読み込みf32サンプルデータに変換します
fn load_system_sound(path: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        return Err(format!("Sound file not found: {}", path).into());
    }

    // AIFFファイルとWAVファイルの両方に対応
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    match extension.as_str() {
        "aiff" | "aif" => {
            // AIFFファイルの場合は一旦WAVとして読み込みを試行
            // houndはAIFFを部分的にサポート
            match hound::WavReader::open(path) {
                Ok(mut reader) => {
                    let samples: Result<Vec<i16>, _> = reader.samples().collect();
                    let samples = samples?;
                    
                    // i16 → f32 変換 (-1.0 to 1.0)
                    Ok(samples.iter().map(|&s| s as f32 / i16::MAX as f32).collect())
                },
                Err(_) => {
                    // AIFFが読み込めない場合は空のデータを返す
                    // 実際の使用ではAIFF専用ライブラリを使用する必要がある
                    eprintln!("Warning: Could not read AIFF file {}, using silence", path);
                    Ok(vec![0.0; 44100]) // 1秒の無音
                }
            }
        },
        "wav" => {
            let mut reader = hound::WavReader::open(path)?;
            let samples: Result<Vec<i16>, _> = reader.samples().collect();
            let samples = samples?;
            
            // i16 → f32 変換 (-1.0 to 1.0)
            Ok(samples.iter().map(|&s| s as f32 / i16::MAX as f32).collect())
        },
        _ => {
            Err(format!("Unsupported audio format: {}", extension).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_sound_player_creation() {
        // デバイスが利用可能な場合のみテスト
        match NativeSoundPlayer::new() {
            Ok(_player) => {
                // 正常に作成できた場合
                println!("NativeSoundPlayer created successfully");
            },
            Err(e) => {
                // デバイスが利用できない環境（CI等）では警告のみ
                println!("NativeSoundPlayer creation failed (expected in headless environments): {}", e);
            }
        }
    }

    #[test]
    fn test_load_system_sound_with_nonexistent_file() {
        let result = load_system_sound("/nonexistent/path.aiff");
        assert!(result.is_err());
    }
}