/// Environment loading helpers.
///
/// Loads environment variables from `.env` if present, or from the file
/// specified by the `VOICE_INPUT_ENV_PATH` environment variable. Any errors
/// during loading are ignored.
pub fn load_env() {
    // 環境変数ファイルを読み込む（EnvConfigの初期化前に実行される）
    if let Ok(path) = std::env::var("VOICE_INPUT_ENV_PATH") {
        dotenvy::from_path(path).ok();
    } else {
        dotenvy::dotenv().ok();
    }
}
