mod assert_cmd {
    use std::process::{Command, Output};

    pub trait CommandCargoExt {
        fn cargo_bin(name: &str) -> Self;
    }

    impl CommandCargoExt for Command {
        fn cargo_bin(name: &str) -> Self {
            let var = format!("CARGO_BIN_EXE_{name}");
            let path = std::env::var(&var).unwrap_or_else(|_| {
                let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                p.push("target/debug");
                p.push(name);
                p.to_string_lossy().into_owned()
            });
            Command::new(path)
        }
    }

    pub trait AssertCmd {
        fn assert(self) -> Assert;
    }

    pub struct Assert {
        output: Output,
    }

    impl Assert {
        pub fn success(self) -> Self {
            assert!(self.output.status.success());
            self
        }

        pub fn stdout<P: Fn(&str) -> bool>(self, pred: P) -> Self {
            let out = String::from_utf8_lossy(&self.output.stdout);
            assert!(pred(&out));
            self
        }
    }

    impl AssertCmd for Command {
        fn assert(mut self) -> Assert {
            let output = self.output().expect("run command");
            Assert { output }
        }
    }

    pub mod prelude {
        pub use super::{AssertCmd, CommandCargoExt};
    }
}

mod predicates {
    pub mod str {
        pub fn contains(s: &str) -> impl Fn(&str) -> bool + '_ {
            move |input: &str| input.contains(s)
        }
    }

    pub mod prelude {}
}

use assert_cmd::prelude::*;
use predicates::str;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::Duration;
use tempfile::TempDir;

fn socket_path(tmp: &TempDir) -> PathBuf {
    tmp.path().join("voice_input.sock")
}

fn configure_ipc_env(cmd: &mut Command, tmp: &TempDir) {
    cmd.env("TMPDIR", tmp.path());
    cmd.env_remove("VOICE_INPUT_SOCKET_DIR");
    cmd.env("VOICE_INPUT_SOCKET_PATH", socket_path(tmp));
}

fn spawn_daemon(tmp: &TempDir) -> Child {
    let mut cmd = Command::cargo_bin("voice_inputd");
    configure_ipc_env(&mut cmd, tmp);
    let socket = socket_path(tmp);
    let child = cmd.spawn().expect("spawn daemon");
    for _ in 0..10 {
        if socket.exists() {
            break;
        }
        sleep(Duration::from_millis(200));
    }
    child
}

fn kill_daemon(tmp: &TempDir, child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_file(socket_path(tmp));
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn list_devices_runs() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut daemon = spawn_daemon(&tmp);

    let mut cmd = Command::cargo_bin("voice_input");
    configure_ipc_env(&mut cmd, &tmp);
    cmd.arg("--list-devices");
    cmd.assert().success().stdout(str::contains(""));

    kill_daemon(&tmp, &mut daemon);
    Ok(())
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn toggle_start_stop() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut daemon = spawn_daemon(&tmp);

    let mut start = Command::cargo_bin("voice_input");
    configure_ipc_env(&mut start, &tmp);
    start.arg("toggle");
    start.assert().success();

    let mut stop = Command::cargo_bin("voice_input");
    configure_ipc_env(&mut stop, &tmp);
    stop.arg("toggle");
    stop.assert().success();

    kill_daemon(&tmp, &mut daemon);
    Ok(())
}
#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn status_returns_idle() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut daemon = spawn_daemon(&tmp);

    let mut cmd = Command::cargo_bin("voice_input");
    configure_ipc_env(&mut cmd, &tmp);
    cmd.arg("status");
    cmd.assert().success().stdout(str::contains("state=Idle"));

    kill_daemon(&tmp, &mut daemon);
    Ok(())
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn health_check_runs() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut daemon = spawn_daemon(&tmp);

    let mut cmd = Command::cargo_bin("voice_input");
    configure_ipc_env(&mut cmd, &tmp);
    cmd.arg("health");
    cmd.assert().success();

    kill_daemon(&tmp, &mut daemon);
    Ok(())
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn dict_add_list_remove() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;

    let mut add_cmd = Command::cargo_bin("voice_input");
    add_cmd
        .args(["dict", "add", "foo", "bar"])
        .env("XDG_DATA_HOME", tmp.path());
    add_cmd.assert().success().stdout(str::contains("Added"));

    let mut list_cmd = Command::cargo_bin("voice_input");
    list_cmd
        .args(["dict", "list"])
        .env("XDG_DATA_HOME", tmp.path());
    list_cmd.assert().success().stdout(str::contains("foo"));

    let mut remove_cmd = Command::cargo_bin("voice_input");
    remove_cmd
        .args(["dict", "remove", "foo"])
        .env("XDG_DATA_HOME", tmp.path());
    remove_cmd
        .assert()
        .success()
        .stdout(str::contains("Removed"));

    Ok(())
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn config_set_moves_dict() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;

    let data_home = tmp.path();
    let default_dict = data_home.join("voice_input/dictionary.json");
    let new_path = data_home.join("shared/dict.json");

    // create dictionary at default location
    let mut add = Command::cargo_bin("voice_input");
    add.args(["dict", "add", "foo", "bar"])
        .env("XDG_DATA_HOME", data_home);
    add.assert().success();

    assert!(default_dict.exists());

    // change path
    let mut set = Command::cargo_bin("voice_input");
    set.args(["config", "set", "dict-path", new_path.to_str().unwrap()])
        .env("XDG_DATA_HOME", data_home);
    set.assert().success();

    assert!(new_path.exists());
    assert!(default_dict.with_extension("bak").exists());

    Ok(())
}

#[test]
fn test_stack_mode_command_parsing() {
    use clap::Parser;
    use voice_input::cli::Cli;

    // Test CLI parsing without running the actual command
    // This just tests the argument parsing logic
    let args = ["voice_input", "stack-mode", "on"];
    match Cli::try_parse_from(args) {
        Ok(_) => {} // Success - command structure is correct
        Err(e) => panic!("Failed to parse stack-mode on command: {}", e),
    }

    let args = ["voice_input", "stack-mode", "off"];
    match Cli::try_parse_from(args) {
        Ok(_) => {}
        Err(e) => panic!("Failed to parse stack-mode off command: {}", e),
    }
}

#[test]
fn test_paste_command_parsing() {
    use clap::Parser;
    use voice_input::cli::Cli;

    let args = ["voice_input", "paste", "5"];
    match Cli::try_parse_from(args) {
        Ok(_) => {}
        Err(e) => panic!("Failed to parse paste command: {}", e),
    }
}

#[test]
fn test_list_stacks_command_parsing() {
    use clap::Parser;
    use voice_input::cli::Cli;

    let args = ["voice_input", "list-stacks"];
    match Cli::try_parse_from(args) {
        Ok(_) => {}
        Err(e) => panic!("Failed to parse list-stacks command: {}", e),
    }
}

#[test]
fn test_clear_stacks_command_parsing() {
    use clap::Parser;
    use voice_input::cli::Cli;

    let args = ["voice_input", "clear-stacks"];
    match Cli::try_parse_from(args) {
        Ok(_) => {}
        Err(e) => panic!("Failed to parse clear-stacks command: {}", e),
    }
}
