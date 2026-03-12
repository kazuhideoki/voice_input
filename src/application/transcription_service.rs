//! 音声文字起こしを管理するサービス
//!
//! # 責任
//! - 音声データの文字起こし
//! - 辞書変換の適用
//! - 同時実行数の制御

use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;

use crate::domain::dict::{DictRepository, apply_replacements};
use crate::error::{Result, VoiceInputError};
use crate::infrastructure::audio::cpal_backend::AudioData;
use crate::infrastructure::dict::JsonFileDictRepo;
use crate::utils::profiling;
use async_trait::async_trait;

/// 音声文字起こし機能の抽象化
#[async_trait]
pub trait TranscriptionClient: Send + Sync {
    /// 音声データを文字起こし
    async fn transcribe(&self, audio: AudioData, language: &str) -> Result<String>;

    /// 音声データをストリーミングで文字起こしする
    async fn transcribe_streaming(
        &self,
        audio: AudioData,
        language: &str,
        _event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<String> {
        self.transcribe(audio, language).await
    }
}

/// ストリーミング転写イベント
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptionEvent {
    /// 増分テキスト
    Delta(String),
    /// 最終確定テキスト
    Completed(String),
}

/// 転写オプション
#[derive(Clone, Debug)]
pub struct TranscriptionOptions {
    /// 言語設定
    pub language: String,
    /// プロンプト（コンテキスト）
    pub prompt: Option<String>,
}

impl Default for TranscriptionOptions {
    fn default() -> Self {
        Self {
            language: "ja".to_string(),
            prompt: None,
        }
    }
}

/// 転写サービス
pub struct TranscriptionService {
    /// 転写クライアント（抽象化されたインターフェース）
    client: Box<dyn TranscriptionClient>,
    /// 辞書リポジトリ
    dict_repo: Box<dyn DictRepository>,
    /// 同時実行数制限用セマフォ
    semaphore: Arc<Semaphore>,
}

impl TranscriptionService {
    /// 新しいTranscriptionServiceを作成
    pub fn new(
        client: Box<dyn TranscriptionClient>,
        dict_repo: Box<dyn DictRepository>,
        max_concurrent: usize,
    ) -> Self {
        Self {
            client,
            dict_repo,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// デフォルト設定で作成
    pub fn with_default_repo(client: Box<dyn TranscriptionClient>) -> Self {
        Self::new(
            client,
            Box::new(JsonFileDictRepo::new()),
            2, // デフォルトの同時実行数
        )
    }

    /// 音声データを文字起こし
    pub async fn transcribe(
        &self,
        audio: AudioData,
        options: TranscriptionOptions,
    ) -> Result<String> {
        let overall_timer = profiling::Timer::start("transcription.total");

        // セマフォで同時実行数を制限
        let _permit = self.semaphore.acquire().await.map_err(|e| {
            VoiceInputError::SystemError(format!("Semaphore acquire failed: {}", e))
        })?;

        // 転写実行
        let api_timer = profiling::Timer::start("transcription.api");
        let text = self.client.transcribe(audio, &options.language).await?;
        api_timer.log();

        // 辞書変換を適用
        let dict_timer = profiling::Timer::start("transcription.dict");
        let processed = self.apply_dictionary(&text)?;
        if profiling::enabled() {
            dict_timer.log_with(&format!(
                "text_len={} processed_len={}",
                text.len(),
                processed.len()
            ));
        } else {
            dict_timer.log();
        }

        if profiling::enabled() {
            overall_timer.log_with(&format!("processed_len={}", processed.len()));
        } else {
            overall_timer.log();
        }
        Ok(processed)
    }

    /// 音声データをストリーミングで文字起こし
    pub async fn transcribe_streaming(
        &self,
        audio: AudioData,
        options: TranscriptionOptions,
        event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<String> {
        let overall_timer = profiling::Timer::start("transcription.streaming_total");

        let _permit = self.semaphore.acquire().await.map_err(|e| {
            VoiceInputError::SystemError(format!("Semaphore acquire failed: {}", e))
        })?;

        let api_timer = profiling::Timer::start("transcription.streaming_api");
        let text = self
            .client
            .transcribe_streaming(audio, &options.language, event_tx.clone())
            .await?;
        api_timer.log();

        let dict_timer = profiling::Timer::start("transcription.streaming_dict");
        let processed = self.apply_dictionary(&text)?;
        if profiling::enabled() {
            dict_timer.log_with(&format!(
                "text_len={} processed_len={}",
                text.len(),
                processed.len()
            ));
        } else {
            dict_timer.log();
        }

        let _ = event_tx.send(TranscriptionEvent::Completed(processed.clone()));

        if profiling::enabled() {
            overall_timer.log_with(&format!("processed_len={}", processed.len()));
        } else {
            overall_timer.log();
        }

        Ok(processed)
    }

    /// 辞書変換を適用
    fn apply_dictionary(&self, text: &str) -> Result<String> {
        let mut entries = self.dict_repo.load().map_err(|e| {
            VoiceInputError::SystemError(format!("Failed to load dictionary: {}", e))
        })?;

        let result = apply_replacements(text, &mut entries);

        // 変更があった場合は保存
        if entries.iter().any(|e| e.hit > 0) {
            self.dict_repo.save(&entries).map_err(|e| {
                VoiceInputError::SystemError(format!("Failed to save dictionary: {}", e))
            })?;
        }

        Ok(result)
    }

    /// セマフォの現在の利用可能数を取得（デバッグ用）
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::profiling;
    use async_trait::async_trait;
    use scopeguard::guard;
    use std::sync::Mutex;

    /// テスト用のモック転写クライアント
    struct MockTranscriptionClient {
        response: String,
        call_count: Arc<Mutex<usize>>,
    }

    impl MockTranscriptionClient {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
                call_count: Arc::new(Mutex::new(0)),
            }
        }

        #[allow(dead_code)]
        fn get_call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    #[async_trait]
    impl TranscriptionClient for MockTranscriptionClient {
        async fn transcribe(&self, _audio: AudioData, _language: &str) -> Result<String> {
            *self.call_count.lock().unwrap() += 1;
            Ok(self.response.clone())
        }
    }

    /// テスト用のモック辞書リポジトリ
    struct MockDictRepo {
        entries: Vec<crate::domain::dict::WordEntry>,
    }

    impl MockDictRepo {
        fn new() -> Self {
            Self {
                entries: vec![crate::domain::dict::WordEntry {
                    surface: "テスト".to_string(),
                    replacement: "test".to_string(),
                    hit: 0,
                    status: crate::domain::dict::EntryStatus::Active,
                }],
            }
        }
    }

    impl DictRepository for MockDictRepo {
        fn load(&self) -> std::io::Result<Vec<crate::domain::dict::WordEntry>> {
            Ok(self.entries.clone())
        }

        fn save(&self, _entries: &[crate::domain::dict::WordEntry]) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// 辞書変換が転写結果に適用される
    #[tokio::test]
    async fn transcription_applies_dictionary() {
        let client = Box::new(MockTranscriptionClient::new("これはテストです"));
        let dict_repo = Box::new(MockDictRepo::new());
        let service = TranscriptionService::new(client, dict_repo, 1);

        let audio = AudioData {
            bytes: vec![0u8; 100],
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };
        let options = TranscriptionOptions::default();

        let result = service.transcribe(audio, options).await.unwrap();
        assert_eq!(result, "これはtestです");
    }

    /// 転写処理でプロファイルログが出力される
    #[tokio::test]
    async fn profile_log_is_emitted_during_transcription() {
        let _guard = guard((), |_| profiling::clear_enabled_override());
        profiling::set_enabled_override(true);
        profiling::reset_log_count();

        let client = Box::new(MockTranscriptionClient::new("これはテストです"));
        let dict_repo = Box::new(MockDictRepo::new());
        let service = TranscriptionService::new(client, dict_repo, 1);

        let audio = AudioData {
            bytes: vec![0u8; 100],
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };
        let options = TranscriptionOptions::default();

        let _ = service.transcribe(audio, options).await.unwrap();
        assert!(profiling::log_count() > 0);
    }

    /// 同時転写が制限内で完了する
    #[tokio::test]
    async fn concurrent_transcriptions_complete_within_limit() {
        let client = Box::new(MockTranscriptionClient::new("test"));
        let dict_repo = Box::new(MockDictRepo::new());
        let service = Arc::new(TranscriptionService::new(client, dict_repo, 1));

        // 同時に2つのタスクを起動
        let service1 = service.clone();
        let service2 = service.clone();

        let handle1 = tokio::spawn(async move {
            let audio = AudioData {
                bytes: vec![0u8; 100],
                mime_type: "audio/wav",
                file_name: "audio.wav".to_string(),
            };
            let options = TranscriptionOptions::default();
            service1.transcribe(audio, options).await
        });

        let handle2 = tokio::spawn(async move {
            // わずかに遅延させて順序を保証
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            let audio = AudioData {
                bytes: vec![0u8; 100],
                mime_type: "audio/wav",
                file_name: "audio.wav".to_string(),
            };
            let options = TranscriptionOptions::default();
            service2.transcribe(audio, options).await
        });

        // 両方のタスクが完了することを確認
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    /// ストリーミング未実装クライアントでも最終確定イベントを通知できる
    #[tokio::test]
    async fn completed_event_is_emitted_when_streaming_uses_default_trait_path() {
        let client = Box::new(MockTranscriptionClient::new("これはテストです"));
        let dict_repo = Box::new(MockDictRepo::new());
        let service = TranscriptionService::new(client, dict_repo, 1);
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        let audio = AudioData {
            bytes: vec![0u8; 100],
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };
        let options = TranscriptionOptions::default();

        let result = service
            .transcribe_streaming(audio, options, event_tx)
            .await
            .unwrap();
        let event = event_rx.recv().await.expect("event should be emitted");

        assert_eq!(result, "これはtestです");
        assert_eq!(
            event,
            TranscriptionEvent::Completed("これはtestです".to_string())
        );
    }

    /// ストリーミング転写ではdeltaを受け取りながら最終結果に到達できる
    #[tokio::test]
    async fn transcription_service_emits_delta_events_before_completion() {
        struct MockStreamingClient;

        #[async_trait]
        impl TranscriptionClient for MockStreamingClient {
            async fn transcribe(&self, _audio: AudioData, _language: &str) -> Result<String> {
                Ok("これはテストです".to_string())
            }

            async fn transcribe_streaming(
                &self,
                _audio: AudioData,
                _language: &str,
                event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
            ) -> Result<String> {
                let _ = event_tx.send(TranscriptionEvent::Delta("これは".to_string()));
                let _ = event_tx.send(TranscriptionEvent::Delta("テストです".to_string()));
                Ok("これはテストです".to_string())
            }
        }

        let service = TranscriptionService::new(
            Box::new(MockStreamingClient),
            Box::new(MockDictRepo::new()),
            1,
        );
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let audio = AudioData {
            bytes: vec![0u8; 100],
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };
        let options = TranscriptionOptions::default();

        let result = service
            .transcribe_streaming(audio, options, event_tx)
            .await
            .unwrap();

        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        assert_eq!(result, "これはtestです");
        assert_eq!(
            events,
            vec![
                TranscriptionEvent::Delta("これは".to_string()),
                TranscriptionEvent::Delta("テストです".to_string()),
                TranscriptionEvent::Completed("これはtestです".to_string()),
            ]
        );
    }
}
