use crate::utils::config::{EnvConfig, TranscriptionProvider};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, copy},
    path::PathBuf,
};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// 辞書ファイルの保存先。
    pub dict_path: Option<String>,
    /// `.env` より優先する既定の転写バックエンド。
    pub transcription_provider: Option<TranscriptionProvider>,
    /// `.env` より優先する最大録音秒数。
    pub max_secs: Option<u64>,
    /// `.env` より優先するpre-roll長。
    pub pre_roll_ms: Option<u64>,
}

fn data_dir() -> PathBuf {
    let config = EnvConfig::get();
    if let Some(xdg_data_home) = &config.paths.xdg_data_home {
        let dir = xdg_data_home.join("voice_input");
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

    /// デーモンの実行時設定を読み込む。
    ///
    /// 単体テストでは利用者の実設定に依存しないよう未指定設定を返す。
    pub fn load_runtime() -> Self {
        #[cfg(test)]
        {
            AppConfig::default()
        }
        #[cfg(not(test))]
        {
            AppConfig::load()
        }
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

    /// コマンド単位の指定がない録音で使う転写バックエンドを返す。
    pub fn effective_transcription_provider(&self) -> TranscriptionProvider {
        self.transcription_provider
            .unwrap_or_else(|| EnvConfig::get().transcription.provider)
    }

    /// コマンド単位の指定を最優先して転写バックエンドを返す。
    pub fn resolve_transcription_provider(
        &self,
        command_override: Option<TranscriptionProvider>,
    ) -> TranscriptionProvider {
        command_override.unwrap_or_else(|| self.effective_transcription_provider())
    }

    /// 実行時設定を優先した最大録音秒数を返す。
    pub fn effective_max_secs(&self) -> u64 {
        self.max_secs
            .unwrap_or_else(|| EnvConfig::get().recording.max_duration_secs)
    }

    /// コマンド単位の指定を最優先して最大録音秒数を返す。
    pub fn resolve_max_secs(&self, command_override: Option<u64>) -> u64 {
        command_override.unwrap_or_else(|| self.effective_max_secs())
    }

    /// 実行時設定を優先したpre-roll長を返す。
    pub fn effective_pre_roll_ms(&self) -> u64 {
        self.pre_roll_ms
            .unwrap_or_else(|| EnvConfig::get().audio.pre_roll_ms)
    }

    pub fn set_dict_path(&mut self, new_path: PathBuf) -> io::Result<()> {
        self.set_dict_path_with(new_path, |config| config.save())
    }

    /// 既定の転写バックエンドを永続化する。
    pub fn set_transcription_provider(
        &mut self,
        provider: TranscriptionProvider,
    ) -> io::Result<()> {
        self.transcription_provider = Some(provider);
        self.save()
    }

    /// 既定の最大録音秒数を永続化する。
    pub fn set_max_secs(&mut self, secs: u64) -> io::Result<()> {
        self.max_secs = Some(secs);
        self.save()
    }

    /// 既定のpre-roll長を永続化する。
    pub fn set_pre_roll_ms(&mut self, millis: u64) -> io::Result<()> {
        self.pre_roll_ms = Some(millis);
        self.save()
    }

    /// 転写バックエンドの永続設定を削除する。
    pub fn unset_transcription_provider(&mut self) -> io::Result<()> {
        self.transcription_provider = None;
        self.save()
    }

    /// 最大録音秒数の永続設定を削除する。
    pub fn unset_max_secs(&mut self) -> io::Result<()> {
        self.max_secs = None;
        self.save()
    }

    /// pre-roll長の永続設定を削除する。
    pub fn unset_pre_roll_ms(&mut self) -> io::Result<()> {
        self.pre_roll_ms = None;
        self.save()
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
    use crate::utils::config::TranscriptionProvider;
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
            ..AppConfig::default()
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

    /// 旧形式の設定ファイルは実行時設定が未指定として読み込める
    #[test]
    fn legacy_config_without_runtime_fields_is_deserialized() {
        let config: AppConfig =
            serde_json::from_str(r#"{"dict_path":"/tmp/dictionary.json"}"#).unwrap();

        assert_eq!(config.dict_path.as_deref(), Some("/tmp/dictionary.json"));
        assert_eq!(config.transcription_provider, None);
        assert_eq!(config.max_secs, None);
        assert_eq!(config.pre_roll_ms, None);
    }

    /// 実行時設定は設定ファイルへ保存可能な形式で往復できる
    #[test]
    fn runtime_fields_roundtrip_via_json() {
        let config = AppConfig {
            dict_path: None,
            transcription_provider: Some(TranscriptionProvider::GptLiveTranscribe),
            max_secs: Some(90),
            pre_roll_ms: Some(250),
        };

        let json = serde_json::to_string(&config).unwrap();
        let restored: AppConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(
            restored.transcription_provider,
            Some(TranscriptionProvider::GptLiveTranscribe)
        );
        assert_eq!(restored.max_secs, Some(90));
        assert_eq!(restored.pre_roll_ms, Some(250));
    }

    /// コマンド単位の指定は永続設定より優先される
    #[test]
    fn command_overrides_take_priority_over_runtime_config() {
        let config = AppConfig {
            dict_path: None,
            transcription_provider: Some(TranscriptionProvider::GptLiveTranscribe),
            max_secs: Some(90),
            pre_roll_ms: Some(250),
        };

        assert_eq!(
            config.resolve_transcription_provider(Some(TranscriptionProvider::MlxQwen3Asr)),
            TranscriptionProvider::MlxQwen3Asr
        );
        assert_eq!(config.resolve_max_secs(Some(120)), 120);
        assert_eq!(config.effective_pre_roll_ms(), 250);
    }
}
