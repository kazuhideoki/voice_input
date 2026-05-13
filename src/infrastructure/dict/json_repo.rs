//! JSON ファイル版 DictRepository 実装
use crate::application::DictRepository;
#[cfg(test)]
use crate::domain::dict::{DictTerm, DictVariant};
use crate::domain::dict::{DictionaryDocument, EntryStatus, WordEntry};
use crate::infrastructure::config::AppConfig;
use crate::infrastructure::dict::migration;
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
        Ok(self.load_dictionary()?.to_word_entries())
    }

    fn save(&self, all: &[WordEntry]) -> Result<()> {
        let mut document = if self.path.exists() {
            self.load_dictionary()?
        } else {
            DictionaryDocument::from_word_entries(all.to_vec())
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?
        };
        merge_word_entries(&mut document, all)?;
        self.save_dictionary(&document)
    }

    fn load_dictionary(&self) -> Result<DictionaryDocument> {
        migration::load_or_migrate(&self.path)
    }

    fn save_dictionary(&self, document: &DictionaryDocument) -> Result<()> {
        migration::save_document(&self.path, document)?;
        Ok(())
    }
}

fn merge_word_entries(document: &mut DictionaryDocument, entries: &[WordEntry]) -> Result<()> {
    let incoming = DictionaryDocument::from_word_entries(entries.to_vec())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;

    for incoming_term in incoming.terms {
        let Some(term) = document
            .terms
            .iter_mut()
            .find(|term| term.term == incoming_term.term)
        else {
            document.terms.push(incoming_term);
            continue;
        };

        for incoming_variant in incoming_term.variants {
            if let Some(variant) = term
                .variants
                .iter_mut()
                .find(|variant| variant.surface == incoming_variant.surface)
            {
                variant.hit = incoming_variant.hit;
                if term.status == EntryStatus::Active {
                    variant.status = incoming_variant.status;
                }
            } else {
                term.variants.push(incoming_variant);
            }
        }
    }

    Ok(())
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
        let document = DictionaryDocument {
            version: crate::domain::dict::CURRENT_DICTIONARY_VERSION,
            terms: vec![DictTerm {
                term: "bar".into(),
                status: EntryStatus::Active,
                variants: vec![DictVariant {
                    surface: "foo".into(),
                    hit: 1,
                    status: EntryStatus::Active,
                }],
            }],
        };
        repo.save_dictionary(&document).expect("save");
        let loaded = repo.load_dictionary().expect("load");
        assert_eq!(loaded, document);
    }

    /// シンボリックリンクの辞書保存でもリンク自体は維持されてリンク先だけ更新される
    #[test]
    fn save_keeps_symbolic_link_and_updates_target_file() {
        let tmp = TempDir::new().expect("create tempdir");
        let actual_path = tmp.path().join("actual-dictionary.json");
        fs::write(
            &actual_path,
            r#"{
  "version": 2,
  "terms": []
}"#,
        )
        .expect("write initial dictionary");

        let link_path = tmp.path().join("dictionary.json");
        symlink(&actual_path, &link_path).expect("create symlink");

        let repo = JsonFileDictRepo { path: link_path };
        let document = DictionaryDocument {
            version: crate::domain::dict::CURRENT_DICTIONARY_VERSION,
            terms: vec![DictTerm {
                term: "bar".into(),
                status: EntryStatus::Active,
                variants: vec![DictVariant {
                    surface: "foo".into(),
                    hit: 1,
                    status: EntryStatus::Active,
                }],
            }],
        };

        repo.save_dictionary(&document).expect("save");

        assert!(
            fs::symlink_metadata(tmp.path().join("dictionary.json"))
                .expect("stat symlink")
                .file_type()
                .is_symlink()
        );

        let loaded = fs::read_to_string(&actual_path).expect("read actual dictionary");
        assert!(loaded.contains("\"version\": 2"));
        assert!(loaded.contains("\"term\": \"bar\""));
        assert!(loaded.contains("\"surface\": \"foo\""));
    }

    /// フラット保存ではv2固有の空termとterm状態を保持しつつhitを更新できる
    #[test]
    fn save_flat_entries_preserves_terms_and_updates_variant_hits() {
        let (repo, _tmp) = repo_in_tmp();
        let document = DictionaryDocument {
            version: crate::domain::dict::CURRENT_DICTIONARY_VERSION,
            terms: vec![
                DictTerm {
                    term: "OpenAI".into(),
                    status: EntryStatus::Active,
                    variants: vec![DictVariant {
                        surface: "オープンAI".into(),
                        hit: 1,
                        status: EntryStatus::Active,
                    }],
                },
                DictTerm {
                    term: "未登録語".into(),
                    status: EntryStatus::Draft,
                    variants: Vec::new(),
                },
            ],
        };
        repo.save_dictionary(&document).expect("save document");

        repo.save(&[WordEntry {
            surface: "オープンAI".into(),
            replacement: "OpenAI".into(),
            hit: 3,
            status: EntryStatus::Active,
        }])
        .expect("save flat entries");

        let loaded = repo.load_dictionary().expect("load document");
        assert_eq!(loaded.terms.len(), 2);
        let updated = loaded
            .terms
            .iter()
            .find(|term| term.term == "OpenAI")
            .expect("updated term");
        assert_eq!(updated.variants[0].hit, 3);

        let empty_term = loaded
            .terms
            .iter()
            .find(|term| term.term == "未登録語")
            .expect("empty term");
        assert_eq!(empty_term.status, EntryStatus::Draft);
        assert!(empty_term.variants.is_empty());
    }
}
