//! 単語辞書エンティティとリポジトリ抽象 – ドメイン層

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::ops::Range;

/// 現在の辞書ファイル形式バージョン。
pub const CURRENT_DICTIONARY_VERSION: u32 = 2;

/// 1 単語エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordEntry {
    pub surface: String,     // 転写文中の語
    pub replacement: String, // 置換後
    pub hit: u32,            // 使用回数（学習用）
    #[serde(default)]
    pub status: EntryStatus, // 有効 / ドラフト
}

/// 単語エントリの状態
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum EntryStatus {
    /// 置換に利用される
    #[default]
    #[serde(alias = "Active")]
    Active,
    /// 無効状態
    #[serde(alias = "Draft")]
    Draft,
}

impl std::fmt::Display for EntryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryStatus::Active => write!(f, "active"),
            EntryStatus::Draft => write!(f, "draft"),
        }
    }
}

/// 辞書適用時の文字位置対応
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementSpanMapping {
    pub raw_char_range: Range<usize>,
    pub processed_char_range: Range<usize>,
}

/// 辞書適用結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacementOutput {
    pub text: String,
    pub span_mappings: Vec<ReplacementSpanMapping>,
}

/// 辞書ファイル全体。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictionaryDocument {
    pub version: u32,
    pub terms: Vec<DictTerm>,
}

/// 正規化後に出力したい対象語句。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictTerm {
    pub term: String,
    #[serde(default)]
    pub status: EntryStatus,
    #[serde(default)]
    pub variants: Vec<DictVariant>,
}

/// 対象語句へ変換する表記ゆれや誤変換候補。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictVariant {
    pub surface: String,
    #[serde(default)]
    pub hit: u32,
    #[serde(default)]
    pub status: EntryStatus,
}

/// 同じ候補が複数の対象語句へ紐付けられている。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryConflictError {
    pub surface: String,
    pub existing_term: String,
    pub conflicting_term: String,
}

impl fmt::Display for DictionaryConflictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dictionary variant {:?} is assigned to both {:?} and {:?}",
            self.surface, self.existing_term, self.conflicting_term
        )
    }
}

impl Error for DictionaryConflictError {}

impl DictionaryDocument {
    /// 空の v2 辞書を作成する。
    pub fn empty() -> Self {
        Self {
            version: CURRENT_DICTIONARY_VERSION,
            terms: Vec::new(),
        }
    }

    /// 旧形式のフラットなエントリ配列から v2 辞書を作成する。
    pub fn from_word_entries(entries: Vec<WordEntry>) -> Result<Self, DictionaryConflictError> {
        let mut assigned_terms: BTreeMap<String, String> = BTreeMap::new();
        let mut terms: Vec<DictTerm> = Vec::new();

        for entry in entries {
            if let Some(existing_term) = assigned_terms.get(&entry.surface) {
                if existing_term != &entry.replacement {
                    return Err(DictionaryConflictError {
                        surface: entry.surface,
                        existing_term: existing_term.clone(),
                        conflicting_term: entry.replacement,
                    });
                }
            } else {
                assigned_terms.insert(entry.surface.clone(), entry.replacement.clone());
            }

            let term = match terms.iter_mut().find(|term| term.term == entry.replacement) {
                Some(term) => term,
                None => {
                    terms.push(DictTerm {
                        term: entry.replacement.clone(),
                        status: EntryStatus::Active,
                        variants: Vec::new(),
                    });
                    terms.last_mut().expect("term was just pushed")
                }
            };

            if let Some(variant) = term
                .variants
                .iter_mut()
                .find(|variant| variant.surface == entry.surface)
            {
                variant.hit += entry.hit;
                variant.status = entry.status;
            } else {
                term.variants.push(DictVariant {
                    surface: entry.surface,
                    hit: entry.hit,
                    status: entry.status,
                });
            }
        }

        Ok(Self {
            version: CURRENT_DICTIONARY_VERSION,
            terms,
        })
    }

    /// 辞書適用用のフラットな置換エントリへ展開する。
    pub fn to_word_entries(&self) -> Vec<WordEntry> {
        self.terms
            .iter()
            .flat_map(|term| {
                term.variants.iter().map(|variant| WordEntry {
                    surface: variant.surface.clone(),
                    replacement: term.term.clone(),
                    hit: variant.hit,
                    status: if term.status == EntryStatus::Active {
                        variant.status
                    } else {
                        EntryStatus::Draft
                    },
                })
            })
            .collect()
    }
}

/// 与えられた文字列に辞書を適用して置換を行います。
///
/// `entries` の各 `surface` を `replacement` へ置換し、
/// 置換が行われた回数だけ `hit` をインクリメントします。
/// TODO 事前構造化（surface_chars のキャッシュ） や、必要なら Aho-Corasick の導入検討で、辞書サイズ増加時の劣化を防ぐ
pub fn apply_replacements(text: &str, entries: &mut [WordEntry]) -> String {
    apply_replacements_with_mappings(text, entries).text
}

/// 与えられた文字列に辞書を適用し、文字位置対応も返します。
pub fn apply_replacements_with_mappings(
    text: &str,
    entries: &mut [WordEntry],
) -> ReplacementOutput {
    for e in entries
        .iter_mut()
        .filter(|e| e.status == EntryStatus::Active)
    {
        let count = text.matches(&e.surface).count();
        e.hit += count as u32;
    }

    let mut out = String::new();
    let mut i = 0;
    let mut processed_index = 0;
    let mut span_mappings = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    while i < chars.len() {
        let mut replaced = false;
        for e in entries.iter().filter(|e| e.status == EntryStatus::Active) {
            let surface_chars: Vec<char> = e.surface.chars().collect();
            if i + surface_chars.len() <= chars.len()
                && chars[i..i + surface_chars.len()] == surface_chars[..]
            {
                out.push_str(&e.replacement);
                let replacement_len = e.replacement.chars().count();
                span_mappings.push(ReplacementSpanMapping {
                    raw_char_range: i..i + surface_chars.len(),
                    processed_char_range: processed_index..processed_index + replacement_len,
                });
                i += surface_chars.len();
                processed_index += replacement_len;
                replaced = true;
                break;
            }
        }
        if !replaced {
            out.push(chars[i]);
            span_mappings.push(ReplacementSpanMapping {
                raw_char_range: i..i + 1,
                processed_char_range: processed_index..processed_index + 1,
            });
            i += 1;
            processed_index += 1;
        }
    }
    ReplacementOutput {
        text: out,
        span_mappings,
    }
}

/// 辞書エントリを追加または置換する。
pub fn upsert_entry(entries: &mut Vec<WordEntry>, entry: WordEntry) {
    if let Some(existing) = entries
        .iter_mut()
        .find(|existing| existing.surface == entry.surface)
    {
        *existing = entry;
    } else {
        entries.push(entry);
    }
}

/// surface で辞書エントリを削除する。戻り値 true=削除した / false=見つからず
pub fn remove_entry(entries: &mut Vec<WordEntry>, surface: &str) -> bool {
    let len_before = entries.len();
    entries.retain(|entry| entry.surface != surface);
    len_before != entries.len()
}

// === Unit tests ==========================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// 置換時にテキストとヒット数が更新される
    #[test]
    fn replace_updates_hits_and_text() {
        let mut entries = vec![
            WordEntry {
                surface: "foo".into(),
                replacement: "bar".into(),
                hit: 0,
                status: EntryStatus::Active,
            },
            WordEntry {
                surface: "bar".into(),
                replacement: "baz".into(),
                hit: 1,
                status: EntryStatus::Active,
            },
        ];

        let out = apply_replacements("foo bar foo", &mut entries);
        assert_eq!(out, "bar baz bar");
        assert_eq!(entries[0].hit, 2); // foo replaced twice
        assert_eq!(entries[1].hit, 2); // bar appeared once, plus previous 1
    }

    /// 辞書適用時に元文字列と変換後文字列の位置対応を保持できる
    #[test]
    fn replacement_output_keeps_span_mappings() {
        let mut entries = vec![WordEntry {
            surface: "テスト".into(),
            replacement: "test".into(),
            hit: 0,
            status: EntryStatus::Active,
        }];

        let out = apply_replacements_with_mappings("これはテストです", &mut entries);

        assert_eq!(out.text, "これはtestです");
        assert_eq!(
            out.span_mappings,
            vec![
                ReplacementSpanMapping {
                    raw_char_range: 0..1,
                    processed_char_range: 0..1,
                },
                ReplacementSpanMapping {
                    raw_char_range: 1..2,
                    processed_char_range: 1..2,
                },
                ReplacementSpanMapping {
                    raw_char_range: 2..3,
                    processed_char_range: 2..3,
                },
                ReplacementSpanMapping {
                    raw_char_range: 3..6,
                    processed_char_range: 3..7,
                },
                ReplacementSpanMapping {
                    raw_char_range: 6..7,
                    processed_char_range: 7..8,
                },
                ReplacementSpanMapping {
                    raw_char_range: 7..8,
                    processed_char_range: 8..9,
                },
            ]
        );
    }

    /// Draft状態のエントリは置換対象にならない
    #[test]
    fn draft_entries_are_ignored() {
        let mut entries = vec![
            WordEntry {
                surface: "foo".into(),
                replacement: "bar".into(),
                hit: 0,
                status: EntryStatus::Draft,
            },
            WordEntry {
                surface: "bar".into(),
                replacement: "baz".into(),
                hit: 0,
                status: EntryStatus::Active,
            },
        ];

        let out = apply_replacements("foo bar", &mut entries);
        assert_eq!(out, "foo baz");
        // foo should not count because entry is draft
        assert_eq!(entries[0].hit, 0);
        assert_eq!(entries[1].hit, 1);
    }

    /// 同じsurfaceのエントリは置換更新できる
    #[test]
    fn upsert_entry_replaces_existing_entry() {
        let mut entries = vec![WordEntry {
            surface: "foo".into(),
            replacement: "bar".into(),
            hit: 1,
            status: EntryStatus::Active,
        }];

        upsert_entry(
            &mut entries,
            WordEntry {
                surface: "foo".into(),
                replacement: "baz".into(),
                hit: 2,
                status: EntryStatus::Draft,
            },
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].replacement, "baz");
        assert_eq!(entries[0].hit, 2);
        assert_eq!(entries[0].status, EntryStatus::Draft);
    }

    /// surface一致のエントリを削除できる
    #[test]
    fn remove_entry_deletes_matching_surface() {
        let mut entries = vec![
            WordEntry {
                surface: "foo".into(),
                replacement: "bar".into(),
                hit: 0,
                status: EntryStatus::Active,
            },
            WordEntry {
                surface: "baz".into(),
                replacement: "qux".into(),
                hit: 0,
                status: EntryStatus::Active,
            },
        ];

        assert!(remove_entry(&mut entries, "foo"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].surface, "baz");
        assert!(!remove_entry(&mut entries, "missing"));
    }
}
