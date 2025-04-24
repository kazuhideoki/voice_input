use std::io;
use std::process::{Command, Stdio};

/// バックグラウンドに完全デタッチして子プロセスを起動するヘルパ
pub fn spawn_detached<I, S>(mut cmd: Command, args: I) -> io::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}
