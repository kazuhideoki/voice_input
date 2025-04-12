use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, SizedSample};
use std::sync::{Arc, Mutex};
use std::vec::Vec;

pub struct AudioRecorder {
    recording: Arc<Mutex<bool>>,
    samples: Arc<Mutex<Vec<f32>>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        AudioRecorder {
            recording: Arc::new(Mutex::new(false)),
            samples: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start_recording(&self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("Failed to get default input device")?;

        println!("Recording device: {}", device.name().unwrap_or_default());

        let config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get default input config: {}", e))?;

        println!("Default input config: {:?}", config);

        let recording = self.recording.clone();
        let samples = self.samples.clone();

        *recording.lock().unwrap() = true;
        samples.lock().unwrap().clear();

        let err_fn = move |err| {
            eprintln!("An error occurred on the audio stream: {}", err);
        };

        let stream = match config.sample_format() {
            SampleFormat::F32 => self.build_stream::<f32>(
                &device,
                &config.into(),
                err_fn,
                recording.clone(),
                samples.clone(),
            ),
            SampleFormat::I16 => self.build_stream::<i16>(
                &device,
                &config.into(),
                err_fn,
                recording.clone(),
                samples.clone(),
            ),
            SampleFormat::U16 => self.build_stream::<u16>(
                &device,
                &config.into(),
                err_fn,
                recording.clone(),
                samples.clone(),
            ),
            _ => return Err("Unsupported sample format".to_string()),
        };

        let stream = stream.map_err(|e| format!("Failed to build stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to play stream: {}", e))?;

        println!("Recording started");

        Ok(())
    }

    fn build_stream<T>(
        &self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
        recording: Arc<Mutex<bool>>,
        samples: Arc<Mutex<Vec<f32>>>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: Sample<Float = f32> + SizedSample + Send + 'static,
    {
        device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if *recording.lock().unwrap() {
                    let mut samples_lock = samples.lock().unwrap();
                    for &sample in data {
                        samples_lock.push(sample.to_float_sample());
                    }
                }
            },
            err_fn,
            None,
        )
    }

    pub fn stop_recording(&self) {
        *self.recording.lock().unwrap() = false;
        println!("Recording stopped");
    }

    pub fn get_samples(&self) -> Vec<f32> {
        self.samples.lock().unwrap().clone()
    }
}
