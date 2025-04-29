//! 単語辞書エンティティとリポジトリ抽象 – ドメイン層

use serde::{Deserialize, Serialize};
use std::io;

/// 1 単語エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordEntry {
    pub surface: String,     // 転写文中の語
    pub replacement: String, // 置換後
    pub hit: u32,            // 使用回数（学習用）
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
