//! 音声文字起こしを管理するサービス
//!
//! # 責任
//! - 音声データの文字起こし
//! - 辞書変換の適用
//! - 同時実行数の制御

use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::application::traits::TranscriptionClient;
use crate::domain::dict::{DictRepository, apply_replacements};
use crate::error::{Result, VoiceInputError};
use crate::infrastructure::audio::cpal_backend::AudioData;
use crate::infrastructure::dict::JsonFileDictRepo;

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
        // セマフォで同時実行数を制限
        let _permit = self.semaphore.acquire().await.map_err(|e| {
            VoiceInputError::SystemError(format!("Semaphore acquire failed: {}", e))
        })?;

        // 転写実行
        let text = self.client.transcribe(audio, &options.language).await?;

        // 辞書変換を適用
        let processed = self.apply_dictionary(&text)?;

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
    use async_trait::async_trait;
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

    #[tokio::test]
    async fn test_transcribe_with_dictionary() {
        let client = Box::new(MockTranscriptionClient::new("これはテストです"));
        let dict_repo = Box::new(MockDictRepo::new());
        let service = TranscriptionService::new(client, dict_repo, 1);

        let audio = AudioData { bytes: vec![0u8; 100], mime_type: "audio/wav", file_name: "audio.wav".to_string() };
        let options = TranscriptionOptions::default();

        let result = service.transcribe(audio, options).await.unwrap();
        assert_eq!(result, "これはtestです");
    }

    #[tokio::test]
    async fn test_concurrent_limit() {
        let client = Box::new(MockTranscriptionClient::new("test"));
        let dict_repo = Box::new(MockDictRepo::new());
        let service = Arc::new(TranscriptionService::new(client, dict_repo, 1));

        // 同時に2つのタスクを起動
        let service1 = service.clone();
        let service2 = service.clone();

        let handle1 = tokio::spawn(async move {
            let audio = AudioData { bytes: vec![0u8; 100], mime_type: "audio/wav", file_name: "audio.wav".to_string() };
            let options = TranscriptionOptions::default();
            service1.transcribe(audio, options).await
        });

        let handle2 = tokio::spawn(async move {
            // わずかに遅延させて順序を保証
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            let audio = AudioData { bytes: vec![0u8; 100], mime_type: "audio/wav", file_name: "audio.wav".to_string() };
            let options = TranscriptionOptions::default();
            service2.transcribe(audio, options).await
        });

        // 両方のタスクが完了することを確認
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }
}
