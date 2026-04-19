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
    plist_path: PathBuf,
    socket_path: PathBuf,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    build_daemon_path: PathBuf,
    installed_daemon_path: PathBuf,
}

impl ScriptFixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path().to_path_buf();
        let fake_bin_dir = root.join("fake-bin");
        let state_dir = root.join("state");
        let repo_root = root.join("repo");
        let home_dir = root.join("home");
        let plist_path = home_dir.join("Library/LaunchAgents/com.user.voiceinputd.plist");
        let socket_path = root.join("runtime/voice_input.sock");
        let stdout_path = root.join("runtime/voice_inputd.out");
        let stderr_path = root.join("runtime/voice_inputd.err");
        let build_daemon_path = repo_root.join("target/release/voice_inputd");
        let installed_daemon_path =
            home_dir.join("Library/Application Support/voice_input/bin/voice_inputd");

        fs::create_dir_all(&fake_bin_dir)?;
        fs::create_dir_all(&state_dir)?;
        fs::create_dir_all(&repo_root)?;
        fs::create_dir_all(home_dir.join("Library/LaunchAgents"))?;
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
    if [ -x "$VOICE_INPUT_INSTALLED_DAEMON_PATH" ]; then
      "$VOICE_INPUT_INSTALLED_DAEMON_PATH"
    fi
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
            &fake_bin_dir.join("codesign"),
            r#"#!/bin/sh
echo "$@" >> "$FAKE_STATE_DIR/codesign.log"
exit 0
"#,
        )?;

        write_executable(
            &fake_bin_dir.join("pkill"),
            r#"#!/bin/sh
echo "$@" >> "$FAKE_STATE_DIR/pkill.log"
exit 0
"#,
        )?;

        Ok(Self {
            _temp_dir: temp_dir,
            fake_bin_dir,
            state_dir,
            repo_root,
            home_dir,
            plist_path,
            socket_path,
            stdout_path,
            stderr_path,
            build_daemon_path,
            installed_daemon_path,
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
        command.env("VOICE_INPUT_LAUNCH_AGENT_PLIST_PATH", &self.plist_path);
        command.env("VOICE_INPUT_SOCKET_PATH", &self.socket_path);
        command.env("VOICE_INPUT_STDOUT_PATH", &self.stdout_path);
        command.env("VOICE_INPUT_STDERR_PATH", &self.stderr_path);
        command.env(
            "VOICE_INPUT_INSTALLED_DAEMON_PATH",
            &self.installed_daemon_path,
        );
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

/// setup-dev-env は固定配置先を使う LaunchAgent plist を作成する
#[test]
#[cfg(feature = "ci-test")]
fn setup_creates_launch_agent_for_installed_daemon() -> Result<(), Box<dyn Error>> {
    let fixture = ScriptFixture::new()?;

    let setup_output = fixture.command_for_script("setup-dev-env.sh").output()?;
    assert!(
        setup_output.status.success(),
        "setup failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&setup_output.stdout),
        String::from_utf8_lossy(&setup_output.stderr)
    );

    let plist = fs::read_to_string(&fixture.plist_path)?;
    assert!(
        plist.contains(&fixture.installed_daemon_path.display().to_string()),
        "setup should point LaunchAgent to installed daemon path: {plist}"
    );
    assert!(
        plist.contains("<key>RunAtLoad</key>"),
        "setup should enable RunAtLoad: {plist}"
    );
    assert!(
        plist.contains("<key>KeepAlive</key>"),
        "setup should enable KeepAlive: {plist}"
    );

    Ok(())
}

/// setup-dev-env の後に dev-build を実行すると固定配置先へ反映され LaunchAgent で利用可能になる
#[test]
#[cfg(feature = "ci-test")]
fn setup_then_dev_build_installs_daemon_and_bootstraps_launch_agent() -> Result<(), Box<dyn Error>>
{
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

    assert!(
        fixture.build_daemon_path.exists(),
        "release daemon was not built"
    );
    assert!(
        fixture.installed_daemon_path.exists(),
        "installed daemon was not created"
    );
    assert!(
        fixture.socket_path.exists(),
        "launch agent did not make daemon available"
    );

    let launchctl_log = fixture.state_log("launchctl.log");
    assert!(
        launchctl_log.contains("bootstrap"),
        "dev-build should bootstrap LaunchAgent when not loaded: {launchctl_log}"
    );
    assert!(
        !launchctl_log.contains("bootout"),
        "fresh setup -> dev-build should not boot out LaunchAgent again: {launchctl_log}"
    );

    let codesign_log = fixture.state_log("codesign.log");
    assert!(
        codesign_log.contains(&fixture.installed_daemon_path.display().to_string()),
        "dev-build should sign installed daemon: {codesign_log}"
    );

    Ok(())
}
