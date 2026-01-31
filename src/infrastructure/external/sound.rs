//! 効果音および Apple Music 制御ユーティリティ。
use std::process::{Command, Output};
#[cfg(test)]
use std::sync::OnceLock;
use tokio::task::spawn_blocking;

#[cfg(test)]
type OsaScriptRunner = Box<dyn Fn(String) -> std::io::Result<Output> + Send + Sync>;

#[cfg(test)]
static TEST_OSASCRIPT_RUNNER: OnceLock<OsaScriptRunner> = OnceLock::new();

#[cfg(test)]
fn set_test_osascript_runner(
    runner: impl Fn(String) -> std::io::Result<Output> + Send + Sync + 'static,
) {
    let _ = TEST_OSASCRIPT_RUNNER.set(Box::new(runner));
}

fn run_osascript(script: String) -> std::io::Result<Output> {
    #[cfg(test)]
    if let Some(runner) = TEST_OSASCRIPT_RUNNER.get() {
        // テスト差し替えがある場合のみ使用する必要があるため Option で有無判定する
        return runner(script);
    }
    // テスト差し替えがない場合は本番実装を使う（通常運用では差し替え不要）
    Command::new("osascript").arg("-e").arg(script).output()
}

/// 録音開始を示すサウンドを再生します。
pub fn play_start_sound() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Ping.aiff")
        .spawn();
}

/// 録音停止を示すサウンドを再生します。
pub fn play_stop_sound() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Purr.aiff")
        .spawn();
}

/// 転写完了を示すサウンドを再生します。
pub fn play_transcription_complete_sound() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Glass.aiff")
        .spawn();
}

/// Apple Music を一時停止し、元々再生中だったかを返します。
pub async fn pause_apple_music() -> bool {
    // 直接 Music アプリを操作する - プロセスチェックをバイパス
    let playing_script = r#"
        try
            tell application "Music"
                set was_playing to (player state is playing)
                if was_playing then
                    pause
                end if
                return was_playing
            end tell
        on error
            return false
        end try
    "#;

    // エラーハンドリングを強化
    match spawn_blocking(move || run_osascript(playing_script.to_string())).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                if let Ok(result) = String::from_utf8(output.stdout) {
                    let trimmed = result.trim();
                    // デバッグ用に結果を出力
                    println!("Music pause result: '{}'", trimmed);
                    return trimmed == "true";
                }
            } else {
                // エラー出力がある場合は表示
                if let Ok(err) = String::from_utf8(output.stderr) {
                    if !err.trim().is_empty() {
                        eprintln!("Music pause error: {}", err.trim());
                    }
                }
            }
        }
        Ok(Err(e)) => {
            eprintln!("Failed to execute osascript: {}", e);
        }
        Err(e) => {
            eprintln!("Failed to join osascript task: {}", e);
        }
    }
    false
}

/// Apple Music を再開します。
pub fn resume_apple_music() {
    // 直接 Music アプリを操作する - プロセスチェックをバイパス
    let play_script = r#"
        try
            tell application "Music"
                play
                return true
            end tell
        on error
            return false
        end try
    "#;

    // エラーハンドリングを強化
    std::thread::spawn(move || {
        match std::process::Command::new("osascript")
            .arg("-e")
            .arg(play_script)
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    if let Ok(result) = String::from_utf8(output.stdout) {
                        println!("Music resume result: '{}'", result.trim());
                    }
                } else {
                    // エラー出力がある場合は表示
                    if let Ok(err) = String::from_utf8(output.stderr) {
                        if !err.trim().is_empty() {
                            eprintln!("Music resume error: {}", err.trim());
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to execute osascript: {}", e);
            }
        }
    });
}

#[cfg(all(test, unix))]
mod tests {
    use super::{pause_apple_music, set_test_osascript_runner};
    use std::time::Duration;
    use std::{os::unix::process::ExitStatusExt, process::Output};

    /// osascript 待機中もランタイムが停止しない
    #[tokio::test(flavor = "current_thread")]
    async fn pause_apple_music_yields_while_waiting() {
        set_test_osascript_runner(|_script| {
            std::thread::sleep(Duration::from_millis(100));
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: b"true\n".to_vec(),
                stderr: Vec::new(),
            })
        });

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = tx.send(());
        });

        let pause_task = pause_apple_music();
        tokio::pin!(pause_task);

        let marker_first = tokio::select! {
            _ = &mut pause_task => false,
            _ = rx => true,
        };

        assert!(
            marker_first,
            "marker task should complete before pause_apple_music returns"
        );
        assert!(pause_task.await);
    }
}
