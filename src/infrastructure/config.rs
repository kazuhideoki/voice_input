use crate::utils::config::EnvConfig;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, copy},
    path::PathBuf,
};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub dict_path: Option<String>,
}

fn data_dir() -> PathBuf {
    let config = EnvConfig::get();
    if let Some(xdg_data_home) = &config.xdg_data_home {
        let dir = PathBuf::from(xdg_data_home).join("voice_input");
        fs::create_dir_all(&dir).expect("create data dir");
        return dir;
    }

    let proj =
        ProjectDirs::from("com", "user", "voice_input").expect("cannot resolve platform dirs");
    let dir = proj.data_local_dir();
    fs::create_dir_all(dir).expect("create data dir");
    dir.to_path_buf()
}

fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

pub fn default_dict_path() -> PathBuf {
    data_dir().join("dictionary.json")
}

fn copy_file_contents(source: &PathBuf, destination: &PathBuf) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut reader = fs::File::open(source)?;
    let mut writer = fs::File::create(destination)?;
    copy(&mut reader, &mut writer)?;
    Ok(())
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(f) = fs::File::open(&path) {
            if let Ok(cfg) = serde_json::from_reader(f) {
                return cfg;
            }
        }
        AppConfig::default()
    }

    pub fn save(&self) -> io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        {
            let f = fs::File::create(&tmp)?;
            serde_json::to_writer_pretty(&f, self)?;
        }
        fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn dict_path(&self) -> PathBuf {
        if let Some(p) = &self.dict_path {
            PathBuf::from(p)
        } else {
            default_dict_path()
        }
    }

    pub fn set_dict_path(&mut self, new_path: PathBuf) -> io::Result<()> {
        self.set_dict_path_with(new_path, |config| config.save())
    }

    fn set_dict_path_with<F>(&mut self, new_path: PathBuf, save: F) -> io::Result<()>
    where
        F: FnOnce(&Self) -> io::Result<()>,
    {
        let old = self.dict_path();
        if old != new_path {
            if old.exists() && !new_path.exists() {
                let bak = old.with_extension("bak");
                if bak.exists() {
                    fs::remove_file(&bak)?;
                }
                copy_file_contents(&old, &bak)?;
                copy_file_contents(&old, &new_path)?;
            } else if !new_path.exists() {
                if let Some(parent) = new_path.parent() {
                    fs::create_dir_all(parent)?;
                }
            }
            self.dict_path = Some(new_path.to_string_lossy().to_string());
            save(self)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    /// 辞書パス変更時に旧パスがシンボリックリンクでもリンクを壊さず内容だけ移行できる
    #[test]
    fn set_dict_path_keeps_symbolic_link_and_copies_contents() {
        let tmp = TempDir::new().expect("create tempdir");
        let actual_path = tmp.path().join("shared/source-dictionary.json");
        fs::create_dir_all(actual_path.parent().expect("parent")).expect("create parent");
        let dictionary_json = "[\n  {\n    \"surface\": \"foo\",\n    \"replacement\": \"bar\",\n    \"hit\": 0,\n    \"status\": \"active\"\n  }\n]\n";
        fs::write(&actual_path, dictionary_json).expect("write dictionary");

        let link_path = tmp.path().join("dictionary.json");
        symlink(&actual_path, &link_path).expect("create symlink");

        let new_path = tmp.path().join("migrated/dictionary.json");
        let mut config = AppConfig {
            dict_path: Some(link_path.to_string_lossy().to_string()),
        };

        config
            .set_dict_path_with(new_path.clone(), |_| Ok(()))
            .expect("set dict path");

        assert!(
            fs::symlink_metadata(&link_path)
                .expect("stat original link")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&link_path).expect("read original link"),
            actual_path
        );

        let backup_path = link_path.with_extension("bak");
        assert!(backup_path.exists());
        assert_eq!(
            fs::read_to_string(&backup_path).expect("read backup"),
            dictionary_json
        );
        assert_eq!(
            fs::read_to_string(&new_path).expect("read migrated dictionary"),
            dictionary_json
        );
        assert_eq!(
            config.dict_path.as_deref(),
            Some(new_path.to_string_lossy().as_ref())
        );
    }
}
