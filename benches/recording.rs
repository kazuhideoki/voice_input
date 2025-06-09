use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use voice_input::domain::recorder::Recorder;
use voice_input::infrastructure::audio::{AudioBackend, AudioData};

/// ベンチマーク用のモックAudioBackend
#[derive(Clone)]
struct BenchmarkAudioBackend {
    recording: Arc<AtomicBool>,
    simulated_size: Arc<AtomicUsize>,
}

impl BenchmarkAudioBackend {
    fn new() -> Self {
        Self {
            recording: Arc::new(AtomicBool::new(false)),
            simulated_size: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn set_simulated_size(&self, size: usize) {
        self.simulated_size.store(size, Ordering::SeqCst);
    }
}

impl AudioBackend for BenchmarkAudioBackend {
    fn start_recording(&self) -> Result<(), Box<dyn Error>> {
        self.recording.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop_recording(&self) -> Result<AudioData, Box<dyn Error>> {
        self.recording.store(false, Ordering::SeqCst);
        let size = self.simulated_size.load(Ordering::SeqCst);

        // メモリモードのみサポート
        Ok(AudioData(vec![0u8; size]))
    }

    fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }
}

fn benchmark_recording_modes(c: &mut Criterion) {
    let mut group = c.benchmark_group("recording_modes");

    // 長時間のI/O測定用に設定を調整
    group
        .sample_size(50) // デフォルト100から減らして時間短縮
        .measurement_time(Duration::from_secs(10)); // 測定時間を10秒に設定

    // 異なる録音時間（秒）でのベンチマーク - より大きなサイズも追加
    for duration_secs in [1, 10, 30, 60, 120].iter() {
        // サンプルレート48kHz、16bit、ステレオと仮定してサイズを計算（実際の録音に近い設定）
        let sample_rate = 48000;
        let bytes_per_sample = 2;
        let channels = 2;
        let audio_size = sample_rate * bytes_per_sample * channels * duration_secs;

        // メモリモードのベンチマーク
        group.bench_with_input(
            BenchmarkId::new("memory_mode", duration_secs),
            duration_secs,
            |b, &_duration| {
                let backend = BenchmarkAudioBackend::new();
                backend.set_simulated_size(audio_size);

                b.iter(|| {
                    let mut recorder = Recorder::new(backend.clone());

                    // 録音開始
                    recorder.start().expect("Failed to start recording");

                    // 録音停止とデータ取得
                    let result = recorder.stop().expect("Failed to stop recording");

                    // 結果の検証（black_boxで最適化を防ぐ）
                    assert_eq!(result.0.len(), audio_size);
                    black_box(result);
                });
            },
        );

    }

    group.finish();
}

fn benchmark_memory_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");

    // メモリ割り当てのベンチマーク
    for mb in [1, 10, 50, 100].iter() {
        let size = mb * 1024 * 1024;

        group.bench_with_input(BenchmarkId::new("allocate", mb), &size, |b, &size| {
            b.iter(|| {
                let _data = vec![0u8; size];
            });
        });
    }

    group.finish();
}

fn benchmark_with_monitoring(c: &mut Criterion) {
    use voice_input::monitoring::MemoryMonitor;

    let mut group = c.benchmark_group("with_monitoring");

    // メモリ監視ありとなしの比較
    for duration_secs in [5, 10].iter() {
        let sample_rate = 44100;
        let bytes_per_sample = 2;
        let channels = 1;
        let audio_size = sample_rate * bytes_per_sample * channels * duration_secs;

        // 監視なし
        group.bench_with_input(
            BenchmarkId::new("without_monitor", duration_secs),
            duration_secs,
            |b, &_duration| {
                let backend = BenchmarkAudioBackend::new();
                backend.set_simulated_size(audio_size);

                b.iter(|| {
                    let mut recorder = Recorder::new(backend.clone());
                    recorder.start().unwrap();
                    let _ = recorder.stop().unwrap();
                });
            },
        );

        // 監視あり
        group.bench_with_input(
            BenchmarkId::new("with_monitor", duration_secs),
            duration_secs,
            |b, &_duration| {
                let backend = BenchmarkAudioBackend::new();
                backend.set_simulated_size(audio_size);
                let monitor = Arc::new(MemoryMonitor::new(500)); // 500MB threshold

                b.iter(|| {
                    let mut recorder =
                        Recorder::new(backend.clone()).with_memory_monitor(monitor.clone());
                    recorder.start().unwrap();
                    let _ = recorder.stop().unwrap();
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_recording_modes,
    benchmark_memory_allocation,
    benchmark_with_monitoring,
);
criterion_main!(benches);
