use crate::domain::dict::{WordEntry, remove_entry, upsert_entry};
use std::io;

/// 辞書永続化 port
pub trait DictRepository: Send + Sync {
    fn load(&self) -> io::Result<Vec<WordEntry>>;
    fn save(&self, all: &[WordEntry]) -> io::Result<()>;
}

/// 辞書更新ユースケース
pub struct DictionaryService {
    repo: Box<dyn DictRepository>,
}

impl DictionaryService {
    /// リポジトリを注入して新しいサービスを作成。
    pub fn new(repo: Box<dyn DictRepository>) -> Self {
        Self { repo }
    }

    /// 辞書一覧を取得。
    pub fn list(&self) -> io::Result<Vec<WordEntry>> {
        self.repo.load()
    }

    /// 追加または更新。
    pub fn upsert(&self, entry: WordEntry) -> io::Result<()> {
        let mut list = self.repo.load()?;
        upsert_entry(&mut list, entry);
        self.repo.save(&list)
    }

    /// surface で削除。戻り値 true=削除した / false=見つからず
    pub fn delete(&self, surface: &str) -> io::Result<bool> {
        let mut list = self.repo.load()?;
        let deleted = remove_entry(&mut list, surface);
        if deleted {
            self.repo.save(&list)?;
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dict::EntryStatus;
    use std::sync::Mutex;

    struct InMemoryDictRepo {
        entries: Mutex<Vec<WordEntry>>,
    }

    impl InMemoryDictRepo {
        fn new(entries: Vec<WordEntry>) -> Self {
            Self {
                entries: Mutex::new(entries),
            }
        }
    }

    impl DictRepository for InMemoryDictRepo {
        fn load(&self) -> io::Result<Vec<WordEntry>> {
            Ok(self.entries.lock().unwrap().clone())
        }

        fn save(&self, all: &[WordEntry]) -> io::Result<()> {
            *self.entries.lock().unwrap() = all.to_vec();
            Ok(())
        }
    }

    /// upsertで追加と更新ができる
    #[test]
    fn upsert_adds_and_updates_entries() {
        let service = DictionaryService::new(Box::new(InMemoryDictRepo::new(Vec::new())));

        service
            .upsert(WordEntry {
                surface: "foo".into(),
                replacement: "bar".into(),
                hit: 0,
                status: EntryStatus::Active,
            })
            .expect("upsert add");

        service
            .upsert(WordEntry {
                surface: "foo".into(),
                replacement: "baz".into(),
                hit: 2,
                status: EntryStatus::Active,
            })
            .expect("upsert update");

        let loaded = service.list().expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].surface, "foo");
        assert_eq!(loaded[0].replacement, "baz");
        assert_eq!(loaded[0].hit, 2);
    }

    /// deleteでエントリが削除される
    #[test]
    fn delete_removes_entry() {
        let service = DictionaryService::new(Box::new(InMemoryDictRepo::new(vec![WordEntry {
            surface: "foo".into(),
            replacement: "bar".into(),
            hit: 0,
            status: EntryStatus::Active,
        }])));

        assert!(service.delete("foo").expect("delete existing"));
        assert!(!service.delete("foo").expect("delete missing"));
        assert!(service.list().expect("load").is_empty());
    }
}
