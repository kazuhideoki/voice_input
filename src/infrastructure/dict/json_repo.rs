//! JSON ファイル版 DictRepository 実装
use crate::domain::dict::{DictRepository, WordEntry};
use directories::ProjectDirs;
use serde_json::{from_reader, to_writer_pretty};
use std::{fs, io::Result, path::PathBuf};

pub struct JsonFileDictRepo {
    path: PathBuf,
}

impl JsonFileDictRepo {
    pub fn new() -> Self {
        // ~/Library/Application Support/voice_input/dictionary.json
        let proj =
            ProjectDirs::from("com", "user", "voice_input").expect("cannot resolve platform dirs");
        let dir = proj.data_local_dir();
        fs::create_dir_all(dir).expect("create data dir");
        Self {
            path: dir.join("dictionary.json"),
        }
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
        let tmp = self.path.with_extension("json.tmp");
        {
            let f = fs::File::create(&tmp)?;
            to_writer_pretty(f, all)?;
        }
        fs::rename(tmp, &self.path)?;
        Ok(())
    }
}
