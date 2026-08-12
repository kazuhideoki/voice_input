#![allow(clippy::disallowed_methods)]

use std::{env, fs, path::Path};

const API_KEY_ENV: &str = "TRANSCRIPTION_API_KEY";
const EMBEDDED_API_KEY_FILE: &str = "transcription_api_key";

fn main() {
    println!("cargo:rerun-if-changed=.env");

    let api_key = api_key_from_dotenv()
        .unwrap_or_else(|| panic!("{API_KEY_ENV} is required in .env at build time"));

    assert!(
        !api_key.contains(['\r', '\n']),
        "{API_KEY_ENV} must not contain a newline"
    );
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR must be set by Cargo");
    fs::write(Path::new(&out_dir).join(EMBEDDED_API_KEY_FILE), api_key)
        .expect("failed to write embedded API key");
}

fn api_key_from_dotenv() -> Option<String> {
    let path = Path::new(".env");
    if !path.is_file() {
        return None;
    }

    let values = dotenvy::from_path_iter(path)
        .unwrap_or_else(|error| panic!("failed to parse .env: {error}"))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("failed to parse .env: {error}"));

    let find = |name: &str| {
        values
            .iter()
            .find_map(|(key, value)| (key == name).then(|| value.clone()))
            .and_then(|value| non_empty(Some(value)))
    };

    find(API_KEY_ENV)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}
