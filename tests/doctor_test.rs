use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use voice_input::ipc::{IpcCmd, IpcError, send_cmd_with_timeout};

fn cli(dir: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_voice_input"));
    command
        .arg("doctor")
        .env("XDG_DATA_HOME", dir.path())
        .env("VOICE_INPUT_SOCKET_PATH", dir.path().join("daemon.sock"))
        .env("VOICE_INPUT_PROFILE", "false")
        .env("VOICE_INPUT_ENV_PATH", dir.path().join("absent.env"));
    command
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn config_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("voice_input/config.json")
}

fn write_config(dir: &TempDir, value: &str) {
    fs::create_dir_all(dir.path().join("voice_input")).unwrap();
    fs::write(config_path(dir), value).unwrap();
}

fn with_daemon(dir: &TempDir, diagnosis: serde_json::Value, ok: bool) -> Output {
    let listener = UnixListener::bind(dir.path().join("daemon.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let started = Instant::now();
        let mut stream = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "CLI did not connect"
            );
            thread::sleep(Duration::from_millis(10));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<IpcCmd>(&line).unwrap(),
            IpcCmd::Doctor
        );
        let response = voice_input::ipc::IpcResp {
            ok,
            msg: diagnosis.to_string(),
        };
        writeln!(stream, "{}", serde_json::to_string(&response).unwrap()).unwrap();
    });
    let output = cli(dir).output().unwrap();
    server.join().unwrap();
    output
}

fn diagnosis(
    dir: &TempDir,
    config: serde_json::Value,
    checks: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({ "config": config, "config_path": config_path(dir), "report": { "checks": checks } })
}

fn ready_checks() -> serde_json::Value {
    serde_json::json!([{ "name": "入力ストリーム", "status": "Ok", "detail": "初期化済み", "action": "" }])
}

/// daemon不在でも未確認項目と復旧手順を表示し、未作成の設定や辞書を作らない
#[test]
fn missing_daemon_reports_actions_without_creating_files() {
    let dir = TempDir::new().unwrap();
    let output = cli(&dir).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("[FAIL] デーモン"));
    assert!(text.contains("[未確認] マイク権限"));
    assert!(text.contains("restart-app-bundle.sh"));
    assert!(!dir.path().join("voice_input").exists());
}

/// 壊れた設定は既定値で隠さず、daemon診断を続けて不足条件をすべて表示する
#[test]
fn invalid_config_does_not_prevent_daemon_diagnosis() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "{invalid JSON");
    let output = with_daemon(
        &dir,
        diagnosis(&dir, serde_json::json!({}), ready_checks()),
        true,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("[FAIL] 設定"));
    assert!(stdout(&output).contains("[OK] 入力ストリーム"));
    assert_eq!(
        fs::read_to_string(config_path(&dir)).unwrap(),
        "{invalid JSON"
    );
}

/// 不正な録音設定とホットキーは利用不可となり、修正方法が表示される
#[test]
fn invalid_recording_settings_and_hotkey_are_reported() {
    let dir = TempDir::new().unwrap();
    write_config(
        &dir,
        r#"{"max_secs":0,"push_to_talk_enabled":true,"push_to_talk_hotkey":"opt+invalid"}"#,
    );
    let output = cli(&dir).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("[FAIL] 設定"));
    assert!(stdout(&output).contains("[FAIL] Push-to-talk"));
}

/// 必須項目が揃えば成功し、警告だけでは失敗にしない
#[test]
fn ready_daemon_returns_success() {
    let dir = TempDir::new().unwrap();
    let output = with_daemon(
        &dir,
        diagnosis(&dir, serde_json::json!({}), ready_checks()),
        true,
    );
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(stdout(&output).contains("利用可能（事前確認"));
}

/// 複数の不足条件は最初の失敗で打ち切らず対処方法とともに表示する
#[test]
fn all_daemon_failures_are_printed() {
    let dir = TempDir::new().unwrap();
    let checks = serde_json::json!([
        { "name": "マイク権限", "status": "Fail", "detail": "拒否", "action": "マイクを許可" },
        { "name": "OpenAI API", "status": "Fail", "detail": "HTTP 401", "action": "再ビルド" }
    ]);
    let output = with_daemon(&dir, diagnosis(&dir, serde_json::json!({}), checks), false);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("[FAIL] マイク権限"));
    assert!(text.contains("[FAIL] OpenAI API"));
    assert!(text.contains("対処: マイクを許可"));
    assert!(text.contains("対処: 再ビルド"));
}

/// CLIとdaemonで設定が異なる場合は未反映として再起動を促す
#[test]
fn stale_daemon_settings_require_action() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, r#"{"max_secs":120}"#);
    let output = with_daemon(
        &dir,
        diagnosis(&dir, serde_json::json!({}), ready_checks()),
        true,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("[FAIL] 設定の反映"));
}

/// 旧辞書を検証しても移行・バックアップ・保存を行わない
#[test]
fn legacy_dictionary_is_checked_without_migration() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "{}");
    let path = dir.path().join("voice_input/dictionary.json");
    let content = r#"[{"surface":"ボイス","replacement":"Voice","hit":0}]"#;
    fs::write(&path, content).unwrap();
    let output = with_daemon(
        &dir,
        diagnosis(&dir, serde_json::json!({}), ready_checks()),
        true,
    );
    assert!(output.status.success(), "{}", stdout(&output));
    assert_eq!(fs::read_to_string(&path).unwrap(), content);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 2);
}

/// 辞書が壊れていると、daemonが正常でも利用不可と表示する
#[test]
fn corrupted_dictionary_is_a_blocker() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "{}");
    fs::write(
        dir.path().join("voice_input/dictionary.json"),
        r#"{"version":999,"terms":[]}"#,
    )
    .unwrap();
    let output = with_daemon(
        &dir,
        diagnosis(&dir, serde_json::json!({}), ready_checks()),
        true,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("[FAIL] 辞書"));
}

/// socketが存在しても応答しないdaemonへの診断通信は制限時間内に終了する
#[test]
fn unresponsive_daemon_times_out() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("unresponsive.sock");
    let _listener = UnixListener::bind(&path).unwrap();
    let started = Instant::now();
    let result = send_cmd_with_timeout(&IpcCmd::Doctor, &path, Duration::from_millis(50));
    assert!(matches!(result, Err(IpcError::Timeout)));
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// 未確認項目は成功や利用可能として扱わない
#[test]
fn unknown_required_check_prevents_success() {
    let dir = TempDir::new().unwrap();
    let checks = serde_json::json!([{ "name": "マイク権限", "status": "Unknown", "detail": "未取得", "action": "確認してください" }]);
    let output = with_daemon(&dir, diagnosis(&dir, serde_json::json!({}), checks), false);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("利用可否を確認できません"));
}

/// 壊れた設定リンクを未作成の設定とは扱わない
#[test]
fn dangling_config_symlink_is_reported_and_preserved() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("voice_input")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("missing.json"), config_path(&dir)).unwrap();
    let output = cli(&dir).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("[FAIL] 設定"));
    assert!(
        fs::symlink_metadata(config_path(&dir))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

/// 空の診断結果では必須条件を確認したことにしない
#[test]
fn empty_daemon_diagnosis_prevents_success() {
    let dir = TempDir::new().unwrap();
    let output = with_daemon(
        &dir,
        diagnosis(&dir, serde_json::json!({}), serde_json::json!([])),
        true,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("[FAIL] デーモン診断"));
}
