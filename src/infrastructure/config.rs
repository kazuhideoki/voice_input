use crate::utils::config::EnvConfig;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

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
        let old = self.dict_path();
        if old != new_path {
            if old.exists() && !new_path.exists() {
                if let Some(parent) = new_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let bak = old.with_extension("bak");
                if bak.exists() {
                    fs::remove_file(&bak)?;
                }
                fs::rename(&old, &bak)?;
                fs::copy(&bak, &new_path)?;
            } else if !new_path.exists() {
                if let Some(parent) = new_path.parent() {
                    fs::create_dir_all(parent)?;
                }
            }
            self.dict_path = Some(new_path.to_string_lossy().to_string());
            self.save()?;
        }
        Ok(())
    }
}
