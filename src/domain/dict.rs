//! 単語辞書エンティティとリポジトリ抽象 – ドメイン層

use serde::{Deserialize, Serialize};
use std::io;

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
pub enum EntryStatus {
    /// 置換に利用される
    #[default]
    Active,
    /// 無効状態
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

/// 与えられた文字列に辞書を適用して置換を行います。
///
/// `entries` の各 `surface` を `replacement` へ置換し、
/// 置換が行われた回数だけ `hit` をインクリメントします。
pub fn apply_replacements(text: &str, entries: &mut [WordEntry]) -> String {
    for e in entries
        .iter_mut()
        .filter(|e| e.status == EntryStatus::Active)
    {
        let count = text.matches(&e.surface).count();
        e.hit += count as u32;
    }

    let mut out = String::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    while i < chars.len() {
        let mut replaced = false;
        for e in entries.iter().filter(|e| e.status == EntryStatus::Active) {
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

/// 辞書永続化 I/F
pub trait DictRepository: Send + Sync {
    fn load(&self) -> io::Result<Vec<WordEntry>>;
    fn save(&self, all: &[WordEntry]) -> io::Result<()>;

    /// 追加 or 置換
    fn upsert(&self, entry: WordEntry) -> io::Result<()> {
        let mut list = self.load()?;
        if let Some(e) = list.iter_mut().find(|e| e.surface == entry.surface) {
            *e = entry;
        } else {
            list.push(entry);
        }
        self.save(&list)
    }

    /// surface で削除。戻り値 true=削除した / false=見つからず
    fn delete(&self, surface: &str) -> io::Result<bool> {
        let mut list = self.load()?;
        let len_before = list.len();
        list.retain(|e| e.surface != surface);
        let deleted = len_before != list.len();
        if deleted {
            self.save(&list)?;
        }
        Ok(deleted)
    }
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
}
