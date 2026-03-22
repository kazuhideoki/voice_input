//! JSON ファイル版 DictRepository 実装
use crate::application::DictRepository;
#[cfg(test)]
use crate::domain::dict::EntryStatus;
use crate::domain::dict::WordEntry;
use crate::infrastructure::config::AppConfig;
use serde_json::{from_reader, to_writer_pretty};
use std::{fs, io::Result, path::PathBuf};

pub struct JsonFileDictRepo {
    path: PathBuf,
}

impl JsonFileDictRepo {
    pub fn new() -> Self {
        let cfg = AppConfig::load();
        let path = cfg.dict_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create data dir");
        }
        Self { path }
    }
}

impl Default for JsonFileDictRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl DictRepository for JsonFileDictRepo {
    fn load(&self) -> Result<Vec<WordEntry>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }
        let f = fs::File::open(&self.path)?;
        Ok(from_reader::<_, Vec<WordEntry>>(f)?)
    }

    fn save(&self, all: &[WordEntry]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let f = fs::File::create(&self.path)?;
        to_writer_pretty(f, all)?;
        Ok(())
    }
}

// === Unit tests ==========================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    fn repo_in_tmp() -> (JsonFileDictRepo, TempDir) {
        let tmp = TempDir::new().expect("create tempdir");
        let repo = JsonFileDictRepo {
            path: tmp.path().join("dictionary.json"),
        };
        (repo, tmp)
    }

    /// 辞書ファイルが存在しない場合は空で返る
    #[test]
    fn load_returns_empty_when_file_missing() {
        let (repo, _tmp) = repo_in_tmp();
        let entries = repo.load().expect("load");
        assert!(entries.is_empty());
    }

    /// 保存した辞書を再読込できる
    #[test]
    fn save_and_load_roundtrip() {
        let (repo, _tmp) = repo_in_tmp();
        let list = vec![WordEntry {
            surface: "foo".into(),
            replacement: "bar".into(),
            hit: 1,
            status: EntryStatus::Active,
        }];
        repo.save(&list).expect("save");
        let loaded = repo.load().expect("load");
        assert_eq!(loaded.len(), list.len());
        assert_eq!(loaded[0].surface, list[0].surface);
        assert_eq!(loaded[0].replacement, list[0].replacement);
        assert_eq!(loaded[0].hit, list[0].hit);
    }

    /// シンボリックリンクの辞書保存でもリンク自体は維持されてリンク先だけ更新される
    #[test]
    fn save_keeps_symbolic_link_and_updates_target_file() {
        let tmp = TempDir::new().expect("create tempdir");
        let actual_path = tmp.path().join("actual-dictionary.json");
        fs::write(&actual_path, "[]").expect("write initial dictionary");

        let link_path = tmp.path().join("dictionary.json");
        symlink(&actual_path, &link_path).expect("create symlink");

        let repo = JsonFileDictRepo { path: link_path };
        let list = vec![WordEntry {
            surface: "foo".into(),
            replacement: "bar".into(),
            hit: 1,
            status: EntryStatus::Active,
        }];

        repo.save(&list).expect("save");

        assert!(
            fs::symlink_metadata(tmp.path().join("dictionary.json"))
                .expect("stat symlink")
                .file_type()
                .is_symlink()
        );

        let loaded = fs::read_to_string(&actual_path).expect("read actual dictionary");
        assert!(loaded.contains("\"surface\": \"foo\""));
        assert!(loaded.contains("\"replacement\": \"bar\""));
    }
}
