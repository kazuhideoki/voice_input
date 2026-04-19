#!/bin/bash
# 開発環境クリーンアップスクリプト

set -u

LAUNCH_AGENT_LABEL="${VOICE_INPUT_LAUNCH_AGENT_LABEL:-com.user.voiceinputd}"
LAUNCH_AGENT_TARGET="gui/$(id -u)/${LAUNCH_AGENT_LABEL}"
LAUNCH_AGENT_PLIST_PATH="${VOICE_INPUT_LAUNCH_AGENT_PLIST_PATH:-$HOME/Library/LaunchAgents/${LAUNCH_AGENT_LABEL}.plist}"
LAUNCH_AGENT_BACKUP_PATH="${VOICE_INPUT_LAUNCH_AGENT_BACKUP_PATH:-${LAUNCH_AGENT_PLIST_PATH}.bak}"
WRAPPER_PATH="${VOICE_INPUT_WRAPPER_PATH:-/usr/local/bin/voice_inputd_wrapper}"
SOCKET_PATH="${VOICE_INPUT_SOCKET_PATH:-/tmp/voice_input.sock}"
STDOUT_PATH="${VOICE_INPUT_STDOUT_PATH:-/tmp/voice_inputd.out}"
STDERR_PATH="${VOICE_INPUT_STDERR_PATH:-/tmp/voice_inputd.err}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO_DAEMON_PATH="${REPO_ROOT}/target/release/voice_inputd"
SUDO_BIN="${VOICE_INPUT_SUDO_BIN:-sudo}"
FAILURES=0

echo "🧹 Cleaning up development environment for voice_input..."

launch_agent_loaded() {
    launchctl print "$LAUNCH_AGENT_TARGET" >/dev/null 2>&1
}

mark_failure() {
    FAILURES=$((FAILURES + 1))
}

stop_launch_agent() {
    if ! launch_agent_loaded; then
        echo "LaunchAgent is not loaded."
        return 0
    fi

    echo "Stopping LaunchAgent..."
    if launchctl bootout "$LAUNCH_AGENT_TARGET" 2>/dev/null; then
        echo "LaunchAgent stopped."
        return 0
    fi

    echo "❌ Failed to stop LaunchAgent: $LAUNCH_AGENT_TARGET" >&2
    mark_failure
    return 1
}

read_wrapper_daemon_path() {
    if [ ! -f "$WRAPPER_PATH" ]; then
        return 0
    fi

    awk '/^exec / { print $2; exit }' "$WRAPPER_PATH"
}

stop_daemon_by_pattern() {
    local pattern="$1"

    if [ -z "$pattern" ]; then
        return 0
    fi

    if pkill -f "$pattern" 2>/dev/null; then
        echo "Stopped process matching: $pattern"
    fi
}

remove_file_if_exists() {
    local path="$1"

    if [ ! -e "$path" ]; then
        return 0
    fi

    if rm -f "$path"; then
        echo "Removed: $path"
        return 0
    fi

    echo "❌ Failed to remove: $path" >&2
    mark_failure
    return 1
}

plist_uses_wrapper() {
    if [ ! -f "$LAUNCH_AGENT_PLIST_PATH" ]; then
        return 1
    fi

    grep -Fq "$WRAPPER_PATH" "$LAUNCH_AGENT_PLIST_PATH"
}

restore_or_remove_launch_agent_plist() {
    if [ -f "$LAUNCH_AGENT_BACKUP_PATH" ]; then
        echo "Restoring LaunchAgent plist backup..."
        if mv -f "$LAUNCH_AGENT_BACKUP_PATH" "$LAUNCH_AGENT_PLIST_PATH"; then
            echo "Restored: $LAUNCH_AGENT_PLIST_PATH"
            return 0
        fi

        echo "❌ Failed to restore LaunchAgent backup." >&2
        mark_failure
        return 1
    fi

    if plist_uses_wrapper; then
        echo "Removing wrapper-based LaunchAgent plist..."
        remove_file_if_exists "$LAUNCH_AGENT_PLIST_PATH"
        return $?
    fi

    echo "No LaunchAgent plist changes were necessary."
    return 0
}

remove_wrapper() {
    if [ ! -e "$WRAPPER_PATH" ]; then
        echo "Wrapper script is already absent."
        return 0
    fi

    echo "Removing wrapper script..."
    if rm -f "$WRAPPER_PATH" 2>/dev/null; then
        echo "Removed: $WRAPPER_PATH"
        return 0
    fi

    if "$SUDO_BIN" rm -f "$WRAPPER_PATH"; then
        echo "Removed with sudo: $WRAPPER_PATH"
        return 0
    fi

    echo "❌ Failed to remove wrapper script: $WRAPPER_PATH" >&2
    mark_failure
    return 1
}

stop_launch_agent
stop_daemon_by_pattern "$(read_wrapper_daemon_path)"
stop_daemon_by_pattern "$REPO_DAEMON_PATH"
remove_file_if_exists "$SOCKET_PATH"
remove_file_if_exists "$STDOUT_PATH"
remove_file_if_exists "$STDERR_PATH"
restore_or_remove_launch_agent_plist
remove_wrapper

echo ""
if [ "$FAILURES" -gt 0 ]; then
    echo "❌ Cleanup finished with ${FAILURES} error(s)." >&2
    exit 1
fi

echo "✅ Cleanup complete!"
echo ""
echo "ℹ️  注意: macOS のマイク/アクセシビリティ権限は自動削除されません。"
echo "   システム設定 → プライバシーとセキュリティから手動で整理してください。"
