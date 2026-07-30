//! 単語辞書エンティティとリポジトリ抽象 – ドメイン層

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

/// 現在の辞書ファイル形式バージョン。
pub const CURRENT_DICTIONARY_VERSION: u32 = 2;

/// 1 単語エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordEntry {
    pub surface: String,     // 転写文中の語
    pub replacement: String, // 置換後
    pub hit: u32,            // 使用回数（学習用）
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
    pub variants: Vec<DictVariant>,
}

/// 対象語句へ変換する表記ゆれや誤変換候補。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictVariant {
    pub surface: String,
    #[serde(default)]
    pub hit: u32,
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
            } else {
                term.variants.push(DictVariant {
                    surface: entry.surface,
                    hit: entry.hit,
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
    for e in entries.iter_mut() {
        let count = text.matches(&e.surface).count();
        e.hit += count as u32;
    }

    let mut out = String::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    while i < chars.len() {
        let mut replaced = false;
        for e in entries.iter() {
            let surface_chars: Vec<char> = e.surface.chars().collect();
            if i + surface_chars.len() <= chars.len()
                && chars[i..i + surface_chars.len()] == surface_chars[..]
            {
                out.push_str(&e.replacement);
                i += surface_chars.len();
                replaced = true;
                break;
            }
        }
        if !replaced {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
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
            },
            WordEntry {
                surface: "bar".into(),
                replacement: "baz".into(),
                hit: 1,
            },
        ];

        let out = apply_replacements("foo bar foo", &mut entries);
        assert_eq!(out, "bar baz bar");
        assert_eq!(entries[0].hit, 2); // foo replaced twice
        assert_eq!(entries[1].hit, 2); // bar appeared once, plus previous 1
    }

    /// 全てのエントリが置換対象になる
    #[test]
    fn all_entries_are_applied() {
        let mut entries = vec![
            WordEntry {
                surface: "foo".into(),
                replacement: "bar".into(),
                hit: 0,
            },
            WordEntry {
                surface: "bar".into(),
                replacement: "baz".into(),
                hit: 0,
            },
        ];

        let out = apply_replacements("foo bar", &mut entries);
        assert_eq!(out, "bar baz");
        assert_eq!(entries[0].hit, 1);
        assert_eq!(entries[1].hit, 1);
    }

    /// 同じsurfaceのエントリは置換更新できる
    #[test]
    fn upsert_entry_replaces_existing_entry() {
        let mut entries = vec![WordEntry {
            surface: "foo".into(),
            replacement: "bar".into(),
            hit: 1,
        }];

        upsert_entry(
            &mut entries,
            WordEntry {
                surface: "foo".into(),
                replacement: "baz".into(),
                hit: 2,
            },
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].replacement, "baz");
        assert_eq!(entries[0].hit, 2);
    }

    /// surface一致のエントリを削除できる
    #[test]
    fn remove_entry_deletes_matching_surface() {
        let mut entries = vec![
            WordEntry {
                surface: "foo".into(),
                replacement: "bar".into(),
                hit: 0,
            },
            WordEntry {
                surface: "baz".into(),
                replacement: "qux".into(),
                hit: 0,
            },
        ];

        assert!(remove_entry(&mut entries, "foo"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].surface, "baz");
        assert!(!remove_entry(&mut entries, "missing"));
    }
}
