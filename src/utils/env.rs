#![allow(clippy::disallowed_methods)]

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

    // APIキーはビルド時の埋め込み値だけを利用し、実行時環境には残さない。
    unsafe {
        std::env::remove_var("TRANSCRIPTION_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");
    }
}
