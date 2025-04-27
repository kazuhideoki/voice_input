use std::{
    io,
    process::{Command, Stdio},
};

/// 子プロセスを完全にデタッチしてバックグラウンド実行するヘルパ。
///
/// 標準入出力をすべて `/dev/null` に向けるため、親プロセス終了後も子が残ります。
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
