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
        pub fn contains<'a>(s: &'a str) -> impl Fn(&str) -> bool + 'a {
            move |input: &str| input.contains(s)
        }
    }

    pub mod prelude {}
}

use assert_cmd::prelude::*;
use predicates::str;
use std::fs;
use std::path::Path;
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::Duration;
use tempfile::TempDir;
use voice_input::ipc::socket_path;

fn spawn_daemon(tmp: &TempDir) -> Child {
    let mut cmd = Command::cargo_bin("voice_inputd");
    cmd.env("TMPDIR", tmp.path());
    let child = cmd.spawn().expect("spawn daemon");
    for _ in 0..10 {
        if Path::new(&socket_path()).exists() {
            break;
        }
        sleep(Duration::from_millis(200));
    }
    child
}

fn kill_daemon(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_file(socket_path());
}

#[test]
fn list_devices_runs() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut daemon = spawn_daemon(&tmp);

    let mut cmd = Command::cargo_bin("voice_input");
    cmd.arg("--list-devices").env("TMPDIR", tmp.path());
    cmd.assert().success().stdout(str::contains(""));

    kill_daemon(&mut daemon);
    Ok(())
}

#[test]
fn toggle_start_stop() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut daemon = spawn_daemon(&tmp);

    let mut start = Command::cargo_bin("voice_input");
    start.arg("toggle").env("TMPDIR", tmp.path());
    start.assert().success();

    let mut stop = Command::cargo_bin("voice_input");
    stop.arg("toggle").env("TMPDIR", tmp.path());
    stop.assert().success();

    kill_daemon(&mut daemon);
    Ok(())
}
#[test]
fn status_returns_idle() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut daemon = spawn_daemon(&tmp);

    let mut cmd = Command::cargo_bin("voice_input");
    cmd.arg("status").env("TMPDIR", tmp.path());
    cmd.assert().success().stdout(str::contains("state=Idle"));

    kill_daemon(&mut daemon);
    Ok(())
}

#[test]
fn health_check_runs() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let mut daemon = spawn_daemon(&tmp);

    let mut cmd = Command::cargo_bin("voice_input");
    cmd.arg("health").env("TMPDIR", tmp.path());
    cmd.assert().success();

    kill_daemon(&mut daemon);
    Ok(())
}

#[test]
fn dict_add_list_remove() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;

    let mut add_cmd = Command::cargo_bin("voice_input");
    add_cmd
        .args(["dict", "add", "foo", "bar"])
        .env("XDG_DATA_HOME", tmp.path());
    add_cmd.assert().success().stdout(str::contains("Added"));

    let mut list_cmd = Command::cargo_bin("voice_input");
    list_cmd.args(["dict", "list"]).env("XDG_DATA_HOME", tmp.path());
    list_cmd.assert().success().stdout(str::contains("foo"));

    let mut remove_cmd = Command::cargo_bin("voice_input");
    remove_cmd
        .args(["dict", "remove", "foo"])
        .env("XDG_DATA_HOME", tmp.path());
    remove_cmd.assert().success().stdout(str::contains("Removed"));

    Ok(())
}
