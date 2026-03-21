//! 音声文字起こしを管理するサービス
//!
//! # 責任
//! - 音声データの文字起こし
//! - 辞書変換の適用
//! - 同時実行数の制御

use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;

use crate::domain::dict::{
    DictRepository, ReplacementSpanMapping, apply_replacements_with_mappings,
};
use crate::error::{Result, VoiceInputError};
use crate::infrastructure::audio::cpal_backend::AudioData;
use crate::infrastructure::dict::JsonFileDictRepo;
use crate::infrastructure::external::transcription_log::NonBlockingTranscriptionLogWriter;
use crate::utils::config::EnvConfig;
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

/// 転写トークン単位の信頼度情報
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionToken {
    /// トークン文字列
    pub token: String,
    /// 対数確率
    pub logprob: f64,
    /// 補助指標としての信頼度
    pub confidence: f64,
}

impl TranscriptionToken {
    /// 対数確率からトークン情報を生成
    pub fn new(token: impl Into<String>, logprob: f64) -> Self {
        Self {
            token: token.into(),
            logprob,
            confidence: logprob.exp(),
        }
    }
}

/// 辞書適用前の転写結果
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionOutput {
    /// 生の全文
    pub text: String,
    /// トークン単位の情報
    pub tokens: Vec<TranscriptionToken>,
}

impl TranscriptionOutput {
    /// トークンを持たない転写結果を生成
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tokens: Vec::new(),
        }
    }
}

/// 低信頼語を選択する範囲
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowConfidenceSelection {
    /// 辞書適用後テキスト上の開始文字位置
    pub start_char_index: usize,
    /// 選択する文字数
    pub char_count: usize,
}

/// 最終入力する転写結果
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalizedTranscription {
    /// 実際に入力する文字列
    pub text: String,
    /// 低信頼語の選択計画
    pub low_confidence_selection: Option<LowConfidenceSelection>,
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
    /// トークン情報
    pub tokens: Vec<TranscriptionToken>,
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
    async fn transcribe(&self, audio: AudioData, language: &str) -> Result<TranscriptionOutput>;

    /// 音声データをストリーミングで文字起こしする
    async fn transcribe_streaming(
        &self,
        audio: AudioData,
        language: &str,
        _event_tx: mpsc::UnboundedSender<TranscriptionEvent>,
    ) -> Result<TranscriptionOutput> {
        self.transcribe(audio, language).await
    }
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

const LOW_CONFIDENCE_THRESHOLD: f64 = 0.3;

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

    /// デフォルト設定で作成
    pub fn with_default_repo(client: Box<dyn TranscriptionClient>) -> Self {
        Self::new_with_optional_env_log(
            client,
            Box::new(JsonFileDictRepo::new()),
            EnvConfig::get().recommended_transcription_parallelism(),
        )
    }

    /// デフォルト辞書 + 環境変数ベースのログ設定付きで作成
    pub fn new_with_optional_env_log(
        client: Box<dyn TranscriptionClient>,
        dict_repo: Box<dyn DictRepository>,
        max_concurrent: usize,
    ) -> Self {
        match EnvConfig::get().transcription.log_path.clone() {
            Some(path) => Self::with_log_writer(
                client,
                dict_repo,
                max_concurrent,
                Box::new(NonBlockingTranscriptionLogWriter::new(path)),
            ),
            None => Self::new(client, dict_repo, max_concurrent),
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
        let output = self.client.transcribe(audio, &options.language).await?;
        api_timer.log();

        // 辞書変換を適用
        let dict_timer = profiling::Timer::start("transcription.dict");
        let processed = self.apply_dictionary(&output.text)?;
        if profiling::enabled() {
            dict_timer.log_with(&format!(
                "text_len={} processed_len={}",
                output.text.len(),
                processed.text.len()
            ));
        } else {
            dict_timer.log();
        }

        let finalized = self.build_finalized_transcription(&output, &processed);
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
        let output = self
            .client
            .transcribe_streaming(audio, &options.language, event_tx.clone())
            .await?;
        api_timer.log();

        let dict_timer = profiling::Timer::start("transcription.streaming_dict");
        let processed = self.apply_dictionary(&output.text)?;
        if profiling::enabled() {
            dict_timer.log_with(&format!(
                "text_len={} processed_len={}",
                output.text.len(),
                processed.text.len()
            ));
        } else {
            dict_timer.log();
        }

        let finalized = self.build_finalized_transcription(&output, &processed);
        self.enqueue_transcription_log(&output, &finalized.text);
        let _ = event_tx.send(TranscriptionEvent::Completed(finalized.clone()));

        if profiling::enabled() {
            overall_timer.log_with(&format!("processed_len={}", finalized.text.len()));
        } else {
            overall_timer.log();
        }

        Ok(finalized)
    }

    fn build_finalized_transcription(
        &self,
        output: &TranscriptionOutput,
        processed: &crate::domain::dict::ReplacementOutput,
    ) -> FinalizedTranscription {
        let low_confidence_selection = if EnvConfig::get()
            .transcription
            .low_confidence_selection_enabled
        {
            plan_low_confidence_selection(
                output,
                &processed.span_mappings,
                LOW_CONFIDENCE_THRESHOLD,
            )
        } else {
            None
        };

        FinalizedTranscription {
            text: processed.text.clone(),
            low_confidence_selection,
        }
    }

    /// 辞書変換を適用
    fn apply_dictionary(&self, text: &str) -> Result<crate::domain::dict::ReplacementOutput> {
        let mut entries = self.dict_repo.load().map_err(|e| {
            VoiceInputError::SystemError(format!("Failed to load dictionary: {}", e))
        })?;

        let result = apply_replacements_with_mappings(text, &mut entries);

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
            tokens: output.tokens.clone(),
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

fn plan_low_confidence_selection(
    output: &TranscriptionOutput,
    span_mappings: &[ReplacementSpanMapping],
    threshold: f64,
) -> Option<LowConfidenceSelection> {
    #[derive(Clone, Copy)]
    struct CandidateGroup {
        raw_start: usize,
        raw_end: usize,
        min_confidence: f64,
    }

    let raw_chars: Vec<char> = output.text.chars().collect();
    let mut raw_index = 0;
    let mut current_group: Option<CandidateGroup> = None;
    let mut groups = Vec::new();

    for token in &output.tokens {
        let token_len = token.token.chars().count();
        if token_len == 0 {
            continue;
        }

        let token_chars: Vec<char> = token.token.chars().collect();
        let raw_slice = raw_chars.get(raw_index..raw_index + token_len)?;
        if raw_slice != token_chars.as_slice() {
            return None;
        }

        let token_start = raw_index;
        let token_end = raw_index + token_len;
        raw_index = token_end;

        if token.confidence < threshold {
            current_group = Some(match current_group {
                Some(group) => CandidateGroup {
                    raw_start: group.raw_start,
                    raw_end: token_end,
                    min_confidence: group.min_confidence.min(token.confidence),
                },
                None => CandidateGroup {
                    raw_start: token_start,
                    raw_end: token_end,
                    min_confidence: token.confidence,
                },
            });
        } else if let Some(group) = current_group.take() {
            groups.push(group);
        }
    }

    if let Some(group) = current_group {
        groups.push(group);
    }

    if raw_index != raw_chars.len() {
        return None;
    }

    let selected_group = groups.into_iter().min_by(|lhs, rhs| {
        lhs.min_confidence
            .partial_cmp(&rhs.min_confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(lhs.raw_start.cmp(&rhs.raw_start))
    })?;

    map_raw_range_to_processed(
        selected_group.raw_start,
        selected_group.raw_end,
        span_mappings,
    )
    .map(|(start_char_index, char_count)| LowConfidenceSelection {
        start_char_index,
        char_count,
    })
}

fn map_raw_range_to_processed(
    raw_start: usize,
    raw_end: usize,
    span_mappings: &[ReplacementSpanMapping],
) -> Option<(usize, usize)> {
    let mut processed_start = None;
    let mut processed_end = None;

    for mapping in span_mappings {
        if mapping.raw_char_range.end <= raw_start {
            continue;
        }
        if mapping.raw_char_range.start >= raw_end {
            break;
        }

        let overlap_start = mapping.raw_char_range.start.max(raw_start);
        let overlap_end = mapping.raw_char_range.end.min(raw_end);
        if overlap_start != mapping.raw_char_range.start
            || overlap_end != mapping.raw_char_range.end
        {
            return None;
        }

        if processed_start.is_none() {
            processed_start = Some(mapping.processed_char_range.start);
        }
        processed_end = Some(mapping.processed_char_range.end);
    }

    let start = processed_start?;
    let end = processed_end?;
    (end > start).then_some((start, end - start))
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
        async fn transcribe(
            &self,
            _audio: AudioData,
            _language: &str,
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

        assert_eq!(result.text, "これはtestです");
        assert_eq!(
            event,
            TranscriptionEvent::Completed(FinalizedTranscription {
                text: "これはtestです".to_string(),
                low_confidence_selection: None,
            })
        );
    }

    /// ストリーミング転写ではdeltaを受け取りながら最終結果に到達できる
    #[tokio::test]
    async fn transcription_service_emits_delta_events_before_completion() {
        struct MockStreamingClient;

        #[async_trait]
        impl TranscriptionClient for MockStreamingClient {
            async fn transcribe(
                &self,
                _audio: AudioData,
                _language: &str,
            ) -> Result<TranscriptionOutput> {
                Ok(TranscriptionOutput::from_text(
                    "これはテストです".to_string(),
                ))
            }

            async fn transcribe_streaming(
                &self,
                _audio: AudioData,
                _language: &str,
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
                    low_confidence_selection: None,
                }),
            ]
        );
    }

    /// ログ保存が有効な場合は辞書適用前後とトークン情報を保存要求できる
    #[tokio::test]
    async fn transcription_log_is_enqueued_with_raw_and_processed_text() {
        struct MockClientWithTokens;

        #[async_trait]
        impl TranscriptionClient for MockClientWithTokens {
            async fn transcribe(
                &self,
                _audio: AudioData,
                _language: &str,
            ) -> Result<TranscriptionOutput> {
                Ok(TranscriptionOutput {
                    text: "これはテストです".to_string(),
                    tokens: vec![
                        TranscriptionToken {
                            token: "これは".to_string(),
                            logprob: -0.1,
                            confidence: 0.9048374180359595,
                        },
                        TranscriptionToken {
                            token: "テスト".to_string(),
                            logprob: -1.2,
                            confidence: 0.30119421191220214,
                        },
                    ],
                })
            }
        }

        let log_writer = MockLogWriter::new();
        let recorded_entries = log_writer.entries.clone();
        let service = TranscriptionService::with_log_writer(
            Box::new(MockClientWithTokens),
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
        assert_eq!(
            entries[0].tokens,
            vec![
                TranscriptionToken {
                    token: "これは".to_string(),
                    logprob: -0.1,
                    confidence: 0.9048374180359595,
                },
                TranscriptionToken {
                    token: "テスト".to_string(),
                    logprob: -1.2,
                    confidence: 0.30119421191220214,
                },
            ]
        );
    }

    /// ログ保存が無効な場合は保存要求を行わない
    #[tokio::test]
    async fn transcription_log_is_not_enqueued_when_writer_is_not_configured() {
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

    /// 辞書変換後テキスト上で低信頼語の選択範囲を組み立てられる
    #[test]
    fn low_confidence_selection_uses_processed_text_span() {
        let output = TranscriptionOutput {
            text: "これはテストです".to_string(),
            tokens: vec![
                TranscriptionToken::new("これは", -0.1),
                TranscriptionToken::new("テスト", -3.0),
                TranscriptionToken::new("です", -0.1),
            ],
        };

        let mapping = crate::domain::dict::apply_replacements_with_mappings(
            "これはテストです",
            &mut [crate::domain::dict::WordEntry {
                surface: "テスト".to_string(),
                replacement: "test".to_string(),
                hit: 0,
                status: crate::domain::dict::EntryStatus::Active,
            }],
        );

        let selection = plan_low_confidence_selection(
            &output,
            &mapping.span_mappings,
            LOW_CONFIDENCE_THRESHOLD,
        );

        assert_eq!(
            selection,
            Some(LowConfidenceSelection {
                start_char_index: 3,
                char_count: 4,
            })
        );
    }

    /// 分離した低信頼語が複数あるときは最低confidenceを含む塊を優先する
    #[test]
    fn lowest_confidence_group_is_selected_when_multiple_groups_exist() {
        let output = TranscriptionOutput {
            text: "abcXYZdefUVWghi".to_string(),
            tokens: vec![
                TranscriptionToken::new("abc", -0.1),
                TranscriptionToken::new("XYZ", -1.3),
                TranscriptionToken::new("def", -0.1),
                TranscriptionToken::new("UVW", -3.0),
                TranscriptionToken::new("ghi", -0.1),
            ],
        };

        let mapping =
            crate::domain::dict::apply_replacements_with_mappings("abcXYZdefUVWghi", &mut []);

        let selection = plan_low_confidence_selection(
            &output,
            &mapping.span_mappings,
            LOW_CONFIDENCE_THRESHOLD,
        );

        assert_eq!(
            selection,
            Some(LowConfidenceSelection {
                start_char_index: 9,
                char_count: 3,
            })
        );
    }

    /// 辞書置換の一部分だけが低信頼な場合は過剰選択を避けるため選択しない
    #[test]
    fn partial_overlap_with_dictionary_replacement_is_not_selected() {
        let output = TranscriptionOutput {
            text: "東京都".to_string(),
            tokens: vec![
                TranscriptionToken::new("東", -3.0),
                TranscriptionToken::new("京都", -0.1),
            ],
        };

        let mapping = crate::domain::dict::apply_replacements_with_mappings(
            "東京都",
            &mut [crate::domain::dict::WordEntry {
                surface: "東京都".to_string(),
                replacement: "Tokyo".to_string(),
                hit: 0,
                status: crate::domain::dict::EntryStatus::Active,
            }],
        );

        let selection = plan_low_confidence_selection(
            &output,
            &mapping.span_mappings,
            LOW_CONFIDENCE_THRESHOLD,
        );

        assert_eq!(selection, None);
    }
}
