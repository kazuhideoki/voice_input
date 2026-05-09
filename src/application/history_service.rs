use std::collections::VecDeque;

use crate::domain::transcription::FinalizedTranscription;
use crate::utils::config::TranscriptionProvider;

const DEFAULT_HISTORY_LIMIT: usize = 10;

/// 確定した転写結果の履歴エントリ
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptionHistoryEntry {
    /// 記録時刻
    pub recorded_at: String,
    /// 入力へ使う確定済みテキスト
    pub text: String,
    /// 転写バックエンド
    pub provider: TranscriptionProvider,
    /// 転写モデル
    pub model: Option<String>,
}

/// daemon 起動中だけ保持する転写履歴
pub struct TranscriptionHistoryService {
    limit: usize,
    entries: VecDeque<TranscriptionHistoryEntry>,
}

impl TranscriptionHistoryService {
    /// 最新10件を保持する履歴サービスを作成する
    pub fn new() -> Self {
        Self::with_limit(DEFAULT_HISTORY_LIMIT)
    }

    /// 保持件数を指定して履歴サービスを作成する
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            entries: VecDeque::new(),
        }
    }

    /// 確定した転写結果を履歴へ追加する
    pub fn record_finalized(
        &mut self,
        finalized: &FinalizedTranscription,
        provider: TranscriptionProvider,
        model: Option<&str>,
    ) {
        if self.limit == 0 {
            return;
        }

        self.entries.push_front(TranscriptionHistoryEntry {
            recorded_at: chrono::Utc::now().to_rfc3339(),
            text: finalized.text.clone(),
            provider,
            model: model.map(ToString::to_string),
        });

        while self.entries.len() > self.limit {
            self.entries.pop_back();
        }
    }

    /// 新しい順に履歴を返す
    pub fn list(&self) -> Vec<TranscriptionHistoryEntry> {
        self.entries.iter().cloned().collect()
    }
}

impl Default for TranscriptionHistoryService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finalized(text: &str) -> FinalizedTranscription {
        FinalizedTranscription {
            text: text.to_string(),
            low_confidence_selection: None,
        }
    }

    /// 確定した転写結果は新しい順に保持される
    #[test]
    fn finalized_entries_are_listed_newest_first() {
        let mut history = TranscriptionHistoryService::with_limit(10);

        history.record_finalized(&finalized("first"), TranscriptionProvider::OpenAi4o, None);
        history.record_finalized(
            &finalized("second"),
            TranscriptionProvider::MlxQwen3Asr,
            Some("Qwen/Qwen3-ASR-1.7B"),
        );

        let entries = history.list();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "second");
        assert_eq!(entries[0].provider, TranscriptionProvider::MlxQwen3Asr);
        assert_eq!(entries[0].model.as_deref(), Some("Qwen/Qwen3-ASR-1.7B"));
        assert_eq!(entries[1].text, "first");
    }

    /// 保持件数を超えた履歴は古いものから削除される
    #[test]
    fn oldest_entries_are_pruned_when_limit_is_exceeded() {
        let mut history = TranscriptionHistoryService::with_limit(2);

        history.record_finalized(&finalized("first"), TranscriptionProvider::OpenAi4o, None);
        history.record_finalized(&finalized("second"), TranscriptionProvider::OpenAi4o, None);
        history.record_finalized(&finalized("third"), TranscriptionProvider::OpenAi4o, None);

        let texts = history
            .list()
            .into_iter()
            .map(|entry| entry.text)
            .collect::<Vec<_>>();

        assert_eq!(texts, vec!["third", "second"]);
    }
}
