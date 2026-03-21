use crate::domain::dict::ReplacementSpanMapping;
use serde::{Deserialize, Serialize};

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

/// 辞書変換後テキストに対する低信頼語の選択範囲を組み立てる
pub fn plan_low_confidence_selection(
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
    use crate::domain::dict::{EntryStatus, WordEntry, apply_replacements_with_mappings};

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

        let mapping = apply_replacements_with_mappings(
            "これはテストです",
            &mut [WordEntry {
                surface: "テスト".to_string(),
                replacement: "test".to_string(),
                hit: 0,
                status: EntryStatus::Active,
            }],
        );

        let selection = plan_low_confidence_selection(&output, &mapping.span_mappings, 0.3);

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

        let mapping = apply_replacements_with_mappings("abcXYZdefUVWghi", &mut []);

        let selection = plan_low_confidence_selection(&output, &mapping.span_mappings, 0.3);

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

        let mapping = apply_replacements_with_mappings(
            "東京都",
            &mut [WordEntry {
                surface: "東京都".to_string(),
                replacement: "Tokyo".to_string(),
                hit: 0,
                status: EntryStatus::Active,
            }],
        );

        let selection = plan_low_confidence_selection(&output, &mapping.span_mappings, 0.3);

        assert_eq!(selection, None);
    }
}
