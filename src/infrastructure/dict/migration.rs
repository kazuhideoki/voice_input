use crate::domain::dict::{CURRENT_DICTIONARY_VERSION, DictionaryDocument, WordEntry};
use serde_json::{Value, from_value, to_value, to_writer_pretty};
use std::{
    fs,
    io::{self, Result},
    path::{Path, PathBuf},
};

struct RawDictionary {
    version: u32,
    value: Value,
}

struct Migration {
    from_version: u32,
    to_version: u32,
    apply: fn(Value) -> Result<Value>,
}

const MIGRATIONS: &[Migration] = &[Migration {
    from_version: 1,
    to_version: 2,
    apply: migrate_v1_to_v2,
}];

pub(super) fn load_or_migrate(path: &Path) -> Result<DictionaryDocument> {
    if !path.exists() {
        return Ok(DictionaryDocument::empty());
    }

    let value: Value = serde_json::from_reader(fs::File::open(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let initial_version = detect_version(&value)?;
    let migrated = run_migrations(RawDictionary {
        version: initial_version,
        value,
    })?;
    let document = parse_current_document(migrated.value)?;

    if migrated.version != initial_version {
        backup_dictionary(path, initial_version)?;
        save_document(path, &document)?;
    }

    Ok(document)
}

pub(super) fn save_document(path: &Path, document: &DictionaryDocument) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(path)?;
    to_writer_pretty(file, document)?;
    Ok(())
}

fn detect_version(value: &Value) -> Result<u32> {
    if value.is_array() {
        return Ok(1);
    }

    let Some(object) = value.as_object() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "dictionary must be a JSON array or object",
        ));
    };
    let Some(version) = object.get("version").and_then(Value::as_u64) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "dictionary object is missing numeric version",
        ));
    };
    u32::try_from(version).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "dictionary version is out of range",
        )
    })
}

fn run_migrations(mut raw: RawDictionary) -> Result<RawDictionary> {
    while raw.version < CURRENT_DICTIONARY_VERSION {
        let Some(migration) = MIGRATIONS
            .iter()
            .find(|migration| migration.from_version == raw.version)
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("no dictionary migration from version {}", raw.version),
            ));
        };
        raw.value = (migration.apply)(raw.value)?;
        raw.version = migration.to_version;
    }

    if raw.version > CURRENT_DICTIONARY_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "dictionary version {} is newer than supported version {}",
                raw.version, CURRENT_DICTIONARY_VERSION
            ),
        ));
    }

    Ok(raw)
}

fn migrate_v1_to_v2(value: Value) -> Result<Value> {
    let entries: Vec<WordEntry> =
        from_value(value).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let document = DictionaryDocument::from_word_entries(entries)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    to_value(document).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn parse_current_document(value: Value) -> Result<DictionaryDocument> {
    let document: DictionaryDocument =
        from_value(value).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if document.version != CURRENT_DICTIONARY_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "dictionary version {} did not migrate to {}",
                document.version, CURRENT_DICTIONARY_VERSION
            ),
        ));
    }
    Ok(document)
}

fn backup_dictionary(path: &Path, version: u32) -> Result<PathBuf> {
    let mut backup = PathBuf::from(format!("{}.v{}.bak", path.display(), version));
    if backup.exists() {
        for index in 1.. {
            let candidate = PathBuf::from(format!("{}.v{}.bak.{}", path.display(), version, index));
            if !candidate.exists() {
                backup = candidate;
                break;
            }
        }
    }
    fs::copy(path, &backup)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// v1辞書はv2形式へ逐次マイグレーションされバックアップも残る
    #[test]
    fn legacy_array_migrates_to_versioned_document_with_backup() {
        let tmp = TempDir::new().expect("create tempdir");
        let path = tmp.path().join("dictionary.json");
        fs::write(
            &path,
            r#"[
  {
    "surface": "オープンAI",
    "replacement": "OpenAI",
    "hit": 3,
    "status": "Active"
  },
  {
    "surface": "オープンエーアイ",
    "replacement": "OpenAI",
    "hit": 1,
    "status": "Active"
  }
]"#,
        )
        .expect("write v1 dictionary");

        let document = load_or_migrate(&path).expect("migrate");

        assert_eq!(document.version, CURRENT_DICTIONARY_VERSION);
        assert_eq!(document.terms.len(), 1);
        assert_eq!(document.terms[0].term, "OpenAI");
        assert_eq!(document.terms[0].variants.len(), 2);
        assert!(path.with_file_name("dictionary.json.v1.bak").exists());

        let saved = fs::read_to_string(&path).expect("read migrated dictionary");
        assert!(saved.contains("\"version\": 2"));
        assert!(saved.contains("\"term\": \"OpenAI\""));
    }

    /// 同じ候補が複数の対象語句に割り当てられているv1辞書は失敗する
    #[test]
    fn conflicting_legacy_surfaces_return_invalid_data() {
        let tmp = TempDir::new().expect("create tempdir");
        let path = tmp.path().join("dictionary.json");
        fs::write(
            &path,
            r#"[
  {
    "surface": "foo",
    "replacement": "bar",
    "hit": 0,
    "status": "Active"
  },
  {
    "surface": "foo",
    "replacement": "baz",
    "hit": 0,
    "status": "Active"
  }
]"#,
        )
        .expect("write v1 dictionary");

        let error = load_or_migrate(&path).expect_err("conflict");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!path.with_file_name("dictionary.json.v1.bak").exists());
    }
}
