use crate::utils::config::{
    EnvConfig, TranscriptionProvider, UserSettingOverrides, xdg_data_home_from_env,
};
use directories::ProjectDirs;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write, copy},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// 辞書ファイルの保存先。
    pub dict_path: Option<String>,
    /// `.env` より優先する既定の転写バックエンド。
    pub transcription_provider: Option<TranscriptionProvider>,
    /// `.env` より優先する最大録音秒数。
    pub max_secs: Option<u64>,
    /// `.env` より優先するpre-roll長。
    pub pre_roll_ms: Option<u64>,
    /// `.env` より優先する入力デバイスの優先順位。
    pub input_device_priorities: Option<Vec<String>>,
    /// `.env` より優先する録音開始・停止サウンド設定。
    pub recording_sounds_enabled: Option<bool>,
    /// `.env` より優先する録音状態HUD設定。
    pub recording_hud_enabled: Option<bool>,
    /// `.env` より優先するpush-to-talk設定。
    pub push_to_talk_enabled: Option<bool>,
    /// `.env` より優先するpush-to-talkホットキー。
    pub push_to_talk_hotkey: Option<String>,
    /// `.env` より優先するGPT Transcribeストリーミング直接入力設定。
    pub transcribe_streaming: Option<bool>,
}

static RUNTIME_CONFIG: OnceCell<AppConfig> = OnceCell::new();

fn data_dir() -> PathBuf {
    if let Some(xdg_data_home) = xdg_data_home_from_env() {
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
        Self::load_from_path(&path)
    }

    fn load_from_path(path: &Path) -> Self {
        if let Ok(f) = fs::File::open(path) {
            if let Ok(cfg) = serde_json::from_reader::<_, AppConfig>(f) {
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
            RUNTIME_CONFIG.get_or_init(AppConfig::load).clone()
        }
    }

    /// デーモン起動中に利用する不変の設定スナップショットを初期化する。
    pub fn init_runtime(config: Self) {
        let _ = RUNTIME_CONFIG.set(config);
    }

    /// 環境既定値より優先する項目を環境設定の初期化用に返す。
    pub fn user_setting_overrides(&self) -> UserSettingOverrides {
        UserSettingOverrides {
            transcription_provider: self.transcription_provider,
            max_secs: self.max_secs,
            pre_roll_ms: self.pre_roll_ms,
            input_device_priorities: self.input_device_priorities.clone(),
            recording_sounds_enabled: self.recording_sounds_enabled,
            recording_hud_enabled: self.recording_hud_enabled,
            push_to_talk_enabled: self.push_to_talk_enabled,
            push_to_talk_hotkey: self.push_to_talk_hotkey.clone(),
            transcribe_streaming: self.transcribe_streaming,
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let path = config_path();
        self.save_to_path(&path)
    }

    fn save_to_path(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let destination =
            if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
                fs::canonicalize(path)?
            } else {
                path.to_path_buf()
            };
        let tmp = destination.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            serde_json::to_writer_pretty(&mut f, self)?;
            f.write_all(b"\n")?;
        }
        fs::rename(tmp, destination)?;
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

    pub fn effective_input_device_priorities(&self) -> Vec<String> {
        self.input_device_priorities
            .clone()
            .unwrap_or_else(|| EnvConfig::get().audio.input_device_priorities.clone())
    }

    pub fn effective_recording_sounds_enabled(&self) -> bool {
        self.recording_sounds_enabled
            .unwrap_or_else(|| EnvConfig::get().audio.recording_sounds_enabled)
    }

    pub fn effective_recording_hud_enabled(&self) -> bool {
        self.recording_hud_enabled
            .unwrap_or_else(|| EnvConfig::get().ui.recording_hud_enabled)
    }

    pub fn effective_push_to_talk_enabled(&self) -> bool {
        self.push_to_talk_enabled
            .unwrap_or_else(|| EnvConfig::get().push_to_talk.enabled)
    }

    pub fn effective_push_to_talk_hotkey(&self) -> String {
        self.push_to_talk_hotkey
            .clone()
            .unwrap_or_else(|| EnvConfig::get().push_to_talk.hotkey.clone())
    }

    pub fn effective_transcribe_streaming(&self) -> bool {
        self.transcribe_streaming
            .unwrap_or_else(|| EnvConfig::get().transcription.streaming_enabled)
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

    pub fn set_input_device_priorities(&mut self, priorities: Vec<String>) -> io::Result<()> {
        self.input_device_priorities = Some(priorities);
        self.save()
    }

    pub fn set_recording_sounds_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.recording_sounds_enabled = Some(enabled);
        self.save()
    }

    pub fn set_recording_hud_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.recording_hud_enabled = Some(enabled);
        self.save()
    }

    pub fn set_push_to_talk_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.push_to_talk_enabled = Some(enabled);
        self.save()
    }

    pub fn set_push_to_talk_hotkey(&mut self, hotkey: String) -> io::Result<()> {
        self.push_to_talk_hotkey = Some(hotkey);
        self.save()
    }

    pub fn set_transcribe_streaming(&mut self, enabled: bool) -> io::Result<()> {
        self.transcribe_streaming = Some(enabled);
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

    pub fn unset_input_device_priorities(&mut self) -> io::Result<()> {
        self.input_device_priorities = None;
        self.save()
    }

    pub fn unset_recording_sounds_enabled(&mut self) -> io::Result<()> {
        self.recording_sounds_enabled = None;
        self.save()
    }

    pub fn unset_recording_hud_enabled(&mut self) -> io::Result<()> {
        self.recording_hud_enabled = None;
        self.save()
    }

    pub fn unset_push_to_talk_enabled(&mut self) -> io::Result<()> {
        self.push_to_talk_enabled = None;
        self.save()
    }

    pub fn unset_push_to_talk_hotkey(&mut self) -> io::Result<()> {
        self.push_to_talk_hotkey = None;
        self.save()
    }

    pub fn unset_transcribe_streaming(&mut self) -> io::Result<()> {
        self.transcribe_streaming = None;
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

    /// dotfiles由来の設定リンクを維持したままリンク先を更新できる
    #[test]
    fn saving_config_keeps_symbolic_link_and_updates_target_file() {
        let tmp = TempDir::new().expect("create tempdir");
        let shared_path = tmp.path().join("dotfiles/config.json");
        fs::create_dir_all(shared_path.parent().expect("parent")).expect("create parent");
        fs::write(&shared_path, r#"{"max_secs":90}"#).expect("write shared config");
        let local_path = tmp.path().join("local/config.json");
        fs::create_dir_all(local_path.parent().expect("parent")).expect("create local parent");
        symlink(&shared_path, &local_path).expect("create config symlink");

        let mut config = AppConfig::load_from_path(&local_path);

        assert_eq!(config.max_secs, Some(90));
        config.max_secs = Some(120);
        config.save_to_path(&local_path).expect("save config");

        assert!(
            fs::symlink_metadata(&local_path)
                .expect("stat local config")
                .file_type()
                .is_symlink()
        );
        let saved = AppConfig::load_from_path(&shared_path);
        assert_eq!(saved.max_secs, Some(120));
        assert!(
            fs::read_to_string(&shared_path)
                .expect("read saved config")
                .ends_with('\n')
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
            input_device_priorities: Some(vec!["External Mic".to_string()]),
            recording_sounds_enabled: Some(false),
            recording_hud_enabled: Some(false),
            push_to_talk_enabled: Some(true),
            push_to_talk_hotkey: Some("cmd+space".to_string()),
            transcribe_streaming: Some(true),
        };

        let json = serde_json::to_string(&config).unwrap();
        let restored: AppConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(
            restored.transcription_provider,
            Some(TranscriptionProvider::GptLiveTranscribe)
        );
        assert_eq!(restored.max_secs, Some(90));
        assert_eq!(restored.pre_roll_ms, Some(250));
        assert_eq!(
            restored.input_device_priorities,
            Some(vec!["External Mic".to_string()])
        );
        assert_eq!(restored.recording_sounds_enabled, Some(false));
        assert_eq!(restored.recording_hud_enabled, Some(false));
        assert_eq!(restored.push_to_talk_enabled, Some(true));
        assert_eq!(restored.push_to_talk_hotkey.as_deref(), Some("cmd+space"));
        assert_eq!(restored.transcribe_streaming, Some(true));
    }

    /// コマンド単位の指定は永続設定より優先される
    #[test]
    fn command_overrides_take_priority_over_runtime_config() {
        let config = AppConfig {
            dict_path: None,
            transcription_provider: Some(TranscriptionProvider::GptLiveTranscribe),
            max_secs: Some(90),
            pre_roll_ms: Some(250),
            ..AppConfig::default()
        };

        assert_eq!(
            config.resolve_transcription_provider(Some(TranscriptionProvider::MlxQwen3Asr)),
            TranscriptionProvider::MlxQwen3Asr
        );
        assert_eq!(config.resolve_max_secs(Some(120)), 120);
        assert_eq!(config.effective_pre_roll_ms(), 250);
    }
}
