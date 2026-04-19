use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

struct ScriptFixture {
    _temp_dir: TempDir,
    fake_bin_dir: PathBuf,
    state_dir: PathBuf,
    repo_root: PathBuf,
    home_dir: PathBuf,
    wrapper_path: PathBuf,
    plist_path: PathBuf,
    socket_path: PathBuf,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    daemon_path: PathBuf,
}

impl ScriptFixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path().to_path_buf();
        let fake_bin_dir = root.join("fake-bin");
        let state_dir = root.join("state");
        let repo_root = root.join("repo");
        let home_dir = root.join("home");
        let wrapper_path = root.join("bin/voice_inputd_wrapper");
        let plist_path = home_dir.join("Library/LaunchAgents/com.user.voiceinputd.plist");
        let socket_path = root.join("runtime/voice_input.sock");
        let stdout_path = root.join("runtime/voice_inputd.out");
        let stderr_path = root.join("runtime/voice_inputd.err");
        let daemon_path = repo_root.join("target/release/voice_inputd");

        fs::create_dir_all(&fake_bin_dir)?;
        fs::create_dir_all(&state_dir)?;
        fs::create_dir_all(&repo_root)?;
        fs::create_dir_all(home_dir.join("Library/LaunchAgents"))?;
        fs::create_dir_all(root.join("bin"))?;
        fs::create_dir_all(root.join("runtime"))?;

        write_executable(
            &fake_bin_dir.join("launchctl"),
            r#"#!/bin/sh
echo "$@" >> "$FAKE_STATE_DIR/launchctl.log"
case "$1" in
  print)
    if [ -f "$FAKE_STATE_DIR/launch_agent_loaded" ]; then
      exit 0
    fi
    exit 1
    ;;
  bootout)
    rm -f "$FAKE_STATE_DIR/launch_agent_loaded"
    exit 0
    ;;
  bootstrap|kickstart)
    touch "$FAKE_STATE_DIR/launch_agent_loaded"
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
"#,
        )?;

        write_executable(
            &fake_bin_dir.join("cargo"),
            r#"#!/bin/sh
echo "$@" >> "$FAKE_STATE_DIR/cargo.log"
if [ "$1" = "build" ] && [ "$2" = "--release" ]; then
  mkdir -p "$VOICE_INPUT_REPO_ROOT/target/release"
  cat > "$VOICE_INPUT_REPO_ROOT/target/release/voice_inputd" <<'EOF'
#!/bin/sh
echo "$0 $@" >> "$FAKE_STATE_DIR/daemon.log"
mkdir -p "$(dirname "$VOICE_INPUT_SOCKET_PATH")"
touch "$VOICE_INPUT_SOCKET_PATH"
exit 0
EOF
  chmod +x "$VOICE_INPUT_REPO_ROOT/target/release/voice_inputd"
  exit 0
fi
echo "unexpected cargo invocation: $@" >&2
exit 1
"#,
        )?;

        write_executable(
            &fake_bin_dir.join("nohup"),
            r#"#!/bin/sh
echo "$@" >> "$FAKE_STATE_DIR/nohup.log"
"$@"
"#,
        )?;

        write_executable(
            &fake_bin_dir.join("pkill"),
            r#"#!/bin/sh
echo "$@" >> "$FAKE_STATE_DIR/pkill.log"
exit 0
"#,
        )?;

        write_executable(
            &fake_bin_dir.join("sudo"),
            r#"#!/bin/sh
echo "$@" >> "$FAKE_STATE_DIR/sudo.log"
"$@"
"#,
        )?;

        fs::write(state_dir.join("launch_agent_loaded"), "")?;
        fs::write(
            &plist_path,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.user.voiceinputd</string>
</dict>
</plist>
"#,
        )?;

        Ok(Self {
            _temp_dir: temp_dir,
            fake_bin_dir,
            state_dir,
            repo_root,
            home_dir,
            wrapper_path,
            plist_path,
            socket_path,
            stdout_path,
            stderr_path,
            daemon_path,
        })
    }

    fn command_for_script(&self, script_name: &str) -> Command {
        let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join(script_name);
        let mut command = Command::new("/bin/bash");
        command.arg(script_path);
        command.env(
            "PATH",
            format!(
                "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                self.fake_bin_dir.display()
            ),
        );
        command.env("HOME", &self.home_dir);
        command.env("FAKE_STATE_DIR", &self.state_dir);
        command.env("VOICE_INPUT_REPO_ROOT", &self.repo_root);
        command.env("VOICE_INPUT_WRAPPER_PATH", &self.wrapper_path);
        command.env("VOICE_INPUT_LAUNCH_AGENT_PLIST_PATH", &self.plist_path);
        command.env("VOICE_INPUT_SOCKET_PATH", &self.socket_path);
        command.env("VOICE_INPUT_STDOUT_PATH", &self.stdout_path);
        command.env("VOICE_INPUT_STDERR_PATH", &self.stderr_path);
        command
    }

    fn state_log(&self, file_name: &str) -> String {
        fs::read_to_string(self.state_dir.join(file_name)).unwrap_or_default()
    }
}

fn write_executable(path: &Path, body: &str) -> Result<(), Box<dyn Error>> {
    fs::write(path, body)?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

/// setup-dev-env の後に dev-build を実行すると LaunchAgent に依存せず利用可能になる
#[test]
#[cfg(feature = "ci-test")]
fn setup_then_dev_build_starts_terminal_managed_daemon() -> Result<(), Box<dyn Error>> {
    let fixture = ScriptFixture::new()?;

    let setup_output = fixture.command_for_script("setup-dev-env.sh").output()?;
    assert!(
        setup_output.status.success(),
        "setup failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&setup_output.stdout),
        String::from_utf8_lossy(&setup_output.stderr)
    );

    let build_output = fixture.command_for_script("dev-build.sh").output()?;
    assert!(
        build_output.status.success(),
        "dev-build failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    assert!(fixture.daemon_path.exists(), "release daemon was not built");
    assert!(
        fixture.socket_path.exists(),
        "daemon socket was not created"
    );
    assert!(
        !fixture.wrapper_path.exists(),
        "wrapper should not be required for development flow"
    );

    let launchctl_log = fixture.state_log("launchctl.log");
    assert!(
        !launchctl_log.contains("bootstrap"),
        "setup/dev-build should not bootstrap LaunchAgent: {launchctl_log}"
    );
    assert!(
        !launchctl_log.contains("kickstart"),
        "setup/dev-build should not kickstart LaunchAgent: {launchctl_log}"
    );

    let cargo_log = fixture.state_log("cargo.log");
    assert!(
        cargo_log.contains("build --release"),
        "dev-build should perform a release build: {cargo_log}"
    );

    let nohup_log = fixture.state_log("nohup.log");
    assert!(
        nohup_log.contains(&fixture.daemon_path.display().to_string()),
        "dev-build should start the built daemon directly: {nohup_log}"
    );

    Ok(())
}
