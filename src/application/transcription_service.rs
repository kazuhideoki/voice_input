//! 音声文字起こしを管理するサービス
//!
//! # 責任
//! - 音声データの文字起こし
//! - 辞書変換の適用
//! - 同時実行数の制御

use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;

use crate::application::{AudioData, DictRepository, TranscriptionProvider};
use crate::domain::dict::apply_replacements;
use crate::domain::transcription::{FinalizedTranscription, TranscriptionOutput};
use crate::error::{Result, VoiceInputError};
use crate::utils::profiling;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TranscriptionClientError {
    #[error("transcription client initialization failed: {message}")]
    Initialization { message: String },
    #[error("transcription request failed: {message}")]
    Request { message: String },
}

/// 調査用の転写ログ
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionLogEntry {
    /// 記録時刻
    pub recorded_at: String,
    /// 辞書適用前の全文
    pub raw_text: String,
    /// 辞書適用後の全文
    pub processed_text: String,
}

/// 転写ログの非同期保存要求
pub trait TranscriptionLogWriter: Send + Sync {
    /// 保存要求をキューに積む
    fn enqueue(&self, entry: TranscriptionLogEntry) -> Result<()>;
}

/// 音声文字起こし機能の抽象化
#[async_trait]
pub trait TranscriptionClient: Send + Sync {
    /// 音声データを文字起こし
    async fn transcribe(
        &self,
        audio: AudioData,
        options: &TranscriptionClientOptions,
    ) -> Result<TranscriptionOutput>;

    /// 音声データをストリーミングで文字起こしする
    async fn transcribe_streaming(
        &self,
        audio: AudioData,
        options: &TranscriptionClientOptions,
        _event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<TranscriptionOutput> {
        self.transcribe(audio, options).await
    }
}

/// 転写クライアントへ渡す実行時オプション
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptionClientOptions {
    /// 転写バックエンド
    pub provider: TranscriptionProvider,
}

/// ストリーミング転写イベント
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptionEvent {
    /// 増分テキスト
    Delta(String),
    /// 最終確定テキスト
    Completed(FinalizedTranscription),
}

/// 転写オプション
#[derive(Clone, Debug)]
pub struct TranscriptionOptions {
    /// 転写バックエンド
    pub provider: TranscriptionProvider,
}

impl Default for TranscriptionOptions {
    fn default() -> Self {
        Self {
            provider: crate::application::config_defaults::TRANSCRIPTION_PROVIDER,
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
    /// 調査用ログ保存
    log_writer: Option<Box<dyn TranscriptionLogWriter>>,
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
            log_writer: None,
        }
    }

    /// ログ保存を有効にして作成
    pub fn with_log_writer(
        client: Box<dyn TranscriptionClient>,
        dict_repo: Box<dyn DictRepository>,
        max_concurrent: usize,
        log_writer: Box<dyn TranscriptionLogWriter>,
    ) -> Self {
        Self {
            client,
            dict_repo,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            log_writer: Some(log_writer),
        }
    }

    /// 音声データを文字起こし
    pub async fn transcribe(
        &self,
        audio: AudioData,
        options: TranscriptionOptions,
    ) -> Result<FinalizedTranscription> {
        let overall_timer = profiling::Timer::start("transcription.total");

        // セマフォで同時実行数を制限
        let _permit = self.semaphore.acquire().await.map_err(|e| {
            VoiceInputError::SystemError(format!("Semaphore acquire failed: {}", e))
        })?;

        // 転写実行
        let api_timer = profiling::Timer::start("transcription.api");
        let client_options = TranscriptionClientOptions {
            provider: options.provider,
        };
        let output = self.client.transcribe(audio, &client_options).await?;
        api_timer.log();

        // 辞書変換を適用
        let dict_timer = profiling::Timer::start("transcription.dict");
        let processed = self.apply_dictionary(&output.text)?;
        if profiling::enabled() {
            dict_timer.log_with(&format!(
                "text_len={} processed_len={}",
                output.text.len(),
                processed.len()
            ));
        } else {
            dict_timer.log();
        }

        let finalized = self.build_finalized_transcription(&processed);
        self.enqueue_transcription_log(&output, &finalized.text);

        if profiling::enabled() {
            overall_timer.log_with(&format!("processed_len={}", finalized.text.len()));
        } else {
            overall_timer.log();
        }
        Ok(finalized)
    }

    /// 音声データをストリーミングで文字起こし
    pub async fn transcribe_streaming(
        &self,
        audio: AudioData,
        options: TranscriptionOptions,
        event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<FinalizedTranscription> {
        let overall_timer = profiling::Timer::start("transcription.streaming_total");

        let _permit = self.semaphore.acquire().await.map_err(|e| {
            VoiceInputError::SystemError(format!("Semaphore acquire failed: {}", e))
        })?;

        let api_timer = profiling::Timer::start("transcription.streaming_api");
        let client_options = TranscriptionClientOptions {
            provider: options.provider,
        };
        let output = self
            .client
            .transcribe_streaming(audio, &client_options, event_tx.clone())
            .await?;
        api_timer.log();

        let dict_timer = profiling::Timer::start("transcription.streaming_dict");
        let processed = self.apply_dictionary(&output.text)?;
        if profiling::enabled() {
            dict_timer.log_with(&format!(
                "text_len={} processed_len={}",
                output.text.len(),
                processed.len()
            ));
        } else {
            dict_timer.log();
        }

        let finalized = self.build_finalized_transcription(&processed);
        self.enqueue_transcription_log(&output, &finalized.text);
        let _ = event_tx.send(TranscriptionEvent::Completed(finalized.clone()));

        if profiling::enabled() {
            overall_timer.log_with(&format!("processed_len={}", finalized.text.len()));
        } else {
            overall_timer.log();
        }

        Ok(finalized)
    }

    /// 転写クライアント以外で得た raw 出力へ辞書変換などの後処理を適用する
    pub fn finalize_output(&self, output: TranscriptionOutput) -> Result<FinalizedTranscription> {
        let dict_timer = profiling::Timer::start("transcription.finalize_dict");
        let processed = self.apply_dictionary(&output.text)?;
        if profiling::enabled() {
            dict_timer.log_with(&format!(
                "text_len={} processed_len={}",
                output.text.len(),
                processed.len()
            ));
        } else {
            dict_timer.log();
        }

        let finalized = self.build_finalized_transcription(&processed);
        self.enqueue_transcription_log(&output, &finalized.text);
        Ok(finalized)
    }

    fn build_finalized_transcription(&self, processed: &str) -> FinalizedTranscription {
        FinalizedTranscription {
            text: processed.to_string(),
        }
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

    /// 調査用の転写ログ保存を非同期キューに積む
    fn enqueue_transcription_log(&self, output: &TranscriptionOutput, processed_text: &str) {
        let Some(log_writer) = &self.log_writer else {
            return;
        };

        let entry = TranscriptionLogEntry {
            recorded_at: chrono::Utc::now().to_rfc3339(),
            raw_text: output.text.clone(),
            processed_text: processed_text.to_string(),
        };

        if let Err(error) = log_writer.enqueue(entry) {
            eprintln!("Failed to enqueue transcription log: {}", error);
        }
    }

    /// セマフォの現在の利用可能数を取得（デバッグ用）
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::transcription::{FinalizedTranscription, TranscriptionOutput};
    use crate::utils::config::EnvConfig;
    use crate::utils::profiling;
    use async_trait::async_trait;
    use scopeguard::guard;
    use std::sync::Mutex;

    fn init_env_config() {
        let _ = EnvConfig::init();
    }

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
        async fn transcribe(
            &self,
            _audio: AudioData,
            _options: &TranscriptionClientOptions,
        ) -> Result<TranscriptionOutput> {
            *self.call_count.lock().unwrap() += 1;
            Ok(TranscriptionOutput::from_text(self.response.clone()))
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

    struct MockLogWriter {
        entries: Arc<Mutex<Vec<TranscriptionLogEntry>>>,
    }

    impl MockLogWriter {
        fn new() -> Self {
            Self {
                entries: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl TranscriptionLogWriter for MockLogWriter {
        fn enqueue(&self, entry: TranscriptionLogEntry) -> Result<()> {
            self.entries.lock().unwrap().push(entry);
            Ok(())
        }
    }

    /// 辞書変換が転写結果に適用される
    #[tokio::test]
    async fn transcription_applies_dictionary() {
        init_env_config();
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
        assert_eq!(result.text, "これはtestです");
    }

    /// 転写処理でプロファイルログが出力される
    #[tokio::test]
    async fn profile_log_is_emitted_during_transcription() {
        init_env_config();
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
        init_env_config();
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
        init_env_config();
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

        assert_eq!(result.text, "これはtestです");
        assert_eq!(
            event,
            TranscriptionEvent::Completed(FinalizedTranscription {
                text: "これはtestです".to_string(),
            })
        );
    }

    /// ストリーミング転写ではdeltaを受け取りながら最終結果に到達できる
    #[tokio::test]
    async fn transcription_service_emits_delta_events_before_completion() {
        init_env_config();
        struct MockStreamingClient;

        #[async_trait]
        impl TranscriptionClient for MockStreamingClient {
            async fn transcribe(
                &self,
                _audio: AudioData,
                _options: &TranscriptionClientOptions,
            ) -> Result<TranscriptionOutput> {
                Ok(TranscriptionOutput::from_text(
                    "これはテストです".to_string(),
                ))
            }

            async fn transcribe_streaming(
                &self,
                _audio: AudioData,
                _options: &TranscriptionClientOptions,
                event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
            ) -> Result<TranscriptionOutput> {
                let _ = event_tx.send(TranscriptionEvent::Delta("これは".to_string()));
                let _ = event_tx.send(TranscriptionEvent::Delta("テストです".to_string()));
                Ok(TranscriptionOutput::from_text(
                    "これはテストです".to_string(),
                ))
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

        assert_eq!(result.text, "これはtestです");
        assert_eq!(
            events,
            vec![
                TranscriptionEvent::Delta("これは".to_string()),
                TranscriptionEvent::Delta("テストです".to_string()),
                TranscriptionEvent::Completed(FinalizedTranscription {
                    text: "これはtestです".to_string(),
                }),
            ]
        );
    }

    /// ログ保存が有効な場合は辞書適用前後のテキストを保存要求できる
    #[tokio::test]
    async fn transcription_log_is_enqueued_with_raw_and_processed_text() {
        init_env_config();
        struct MockLoggingClient;

        #[async_trait]
        impl TranscriptionClient for MockLoggingClient {
            async fn transcribe(
                &self,
                _audio: AudioData,
                _options: &TranscriptionClientOptions,
            ) -> Result<TranscriptionOutput> {
                Ok(TranscriptionOutput::from_text("これはテストです"))
            }
        }

        let log_writer = MockLogWriter::new();
        let recorded_entries = log_writer.entries.clone();
        let service = TranscriptionService::with_log_writer(
            Box::new(MockLoggingClient),
            Box::new(MockDictRepo::new()),
            1,
            Box::new(log_writer),
        );

        let audio = AudioData {
            bytes: vec![0u8; 100],
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };

        let result = service
            .transcribe(audio, TranscriptionOptions::default())
            .await
            .unwrap();

        assert_eq!(result.text, "これはtestです");

        let entries = recorded_entries.lock().unwrap().clone();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].raw_text, "これはテストです");
        assert_eq!(entries[0].processed_text, "これはtestです");
    }

    /// ログ保存が無効な場合は保存要求を行わない
    #[tokio::test]
    async fn transcription_log_is_not_enqueued_when_writer_is_not_configured() {
        init_env_config();
        let client = Box::new(MockTranscriptionClient::new("これはテストです"));
        let dict_repo = Box::new(MockDictRepo::new());
        let service = TranscriptionService::new(client, dict_repo, 1);

        let audio = AudioData {
            bytes: vec![0u8; 100],
            mime_type: "audio/wav",
            file_name: "audio.wav".to_string(),
        };

        let result = service
            .transcribe(audio, TranscriptionOptions::default())
            .await
            .unwrap();

        assert_eq!(result.text, "これはtestです");
    }
}
