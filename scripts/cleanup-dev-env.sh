#!/bin/bash
# 開発環境クリーンアップスクリプト

set -eu

LAUNCH_AGENT_LABEL="${VOICE_INPUT_LAUNCH_AGENT_LABEL:-com.user.voiceinputd}"
LAUNCH_AGENT_TARGET="gui/$(id -u)/${LAUNCH_AGENT_LABEL}"
LAUNCH_AGENT_PLIST_PATH="${VOICE_INPUT_LAUNCH_AGENT_PLIST_PATH:-$HOME/Library/LaunchAgents/${LAUNCH_AGENT_LABEL}.plist}"
INSTALLED_DAEMON_PATH="${VOICE_INPUT_INSTALLED_DAEMON_PATH:-$HOME/Library/Application Support/voice_input/bin/voice_inputd}"
SOCKET_PATH="${VOICE_INPUT_SOCKET_PATH:-/tmp/voice_input.sock}"
STDOUT_PATH="${VOICE_INPUT_STDOUT_PATH:-/tmp/voice_inputd.out}"
STDERR_PATH="${VOICE_INPUT_STDERR_PATH:-/tmp/voice_inputd.err}"
FAILURES=0

echo "🧹 Cleaning up development environment for voice_input..."

launch_agent_loaded() {
    launchctl print "$LAUNCH_AGENT_TARGET" >/dev/null 2>&1
}

mark_failure() {
    FAILURES=$((FAILURES + 1))
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

stop_launch_agent_if_loaded() {
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

stop_launch_agent_if_loaded
remove_file_if_exists "$SOCKET_PATH"
remove_file_if_exists "$STDOUT_PATH"
remove_file_if_exists "$STDERR_PATH"
remove_file_if_exists "$LAUNCH_AGENT_PLIST_PATH"
remove_file_if_exists "$INSTALLED_DAEMON_PATH"

echo ""
if [ "$FAILURES" -gt 0 ]; then
    echo "❌ Cleanup finished with ${FAILURES} error(s)." >&2
    exit 1
fi

echo "✅ Cleanup complete!"
echo ""
echo "ℹ️  注意: macOS のマイク/アクセシビリティ権限は自動削除されません。"
echo "   システム設定 → プライバシーとセキュリティから手動で整理してください。"
