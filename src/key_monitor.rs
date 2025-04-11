// src/key_monitor.rs
use device_query::{DeviceQuery, DeviceState, Keycode};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct KeyMonitor {
    callback: Arc<Mutex<dyn Fn(Keycode) + Send + 'static>>,
}

impl KeyMonitor {
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(Keycode) + Send + 'static,
    {
        KeyMonitor {
            callback: Arc::new(Mutex::new(callback)),
        }
    }

    pub fn start_monitoring(&self) -> thread::JoinHandle<()> {
        let device_state = DeviceState::new();
        let callback = Arc::clone(&self.callback);

        thread::spawn(move || {
            let mut last_keys = Vec::new();

            loop {
                let keys = device_state.get_keys();

                // 新しく押されたキーを検出
                for key in &keys {
                    if !last_keys.contains(key) {
                        println!("キーが押されました: {:?}", key);
                        if let Ok(callback) = callback.lock() {
                            callback(*key);
                        }
                    }
                }

                last_keys = keys;
                thread::sleep(Duration::from_millis(10));
            }
        })
    }
}
