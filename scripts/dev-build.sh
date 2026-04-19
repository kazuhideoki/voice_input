#!/bin/bash
# 開発用ビルドスクリプト

set -eu

# Rustc が macOS 15 の一部環境で root 所有の /var/folders/zz/.../T を参照して
# Permission denied になる問題への暫定対応として、書き込み可能な専用 TMPDIR を設定する。
VOICE_INPUT_TMP="/tmp"
mkdir -p "$VOICE_INPUT_TMP"
export TMPDIR="$VOICE_INPUT_TMP"

REPO_ROOT="${VOICE_INPUT_REPO_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
LAUNCH_AGENT_LABEL="${VOICE_INPUT_LAUNCH_AGENT_LABEL:-com.user.voiceinputd}"
LAUNCH_AGENT_TARGET="gui/$(id -u)/${LAUNCH_AGENT_LABEL}"
DAEMON_PATH="${VOICE_INPUT_DAEMON_PATH:-${REPO_ROOT}/target/release/voice_inputd}"
SOCKET_PATH="${VOICE_INPUT_SOCKET_PATH:-/tmp/voice_input.sock}"
STDOUT_PATH="${VOICE_INPUT_STDOUT_PATH:-/tmp/voice_inputd.out}"
STDERR_PATH="${VOICE_INPUT_STDERR_PATH:-/tmp/voice_inputd.err}"

launch_agent_loaded() {
    launchctl print "$LAUNCH_AGENT_TARGET" >/dev/null 2>&1
}

stop_launch_agent_if_loaded() {
    if ! launch_agent_loaded; then
        return 0
    fi

    echo "Stopping LaunchAgent to avoid duplicate voice_inputd processes..."
    if ! launchctl bootout "$LAUNCH_AGENT_TARGET" 2>/dev/null; then
        echo "❌ Failed to stop LaunchAgent: $LAUNCH_AGENT_TARGET" >&2
        exit 1
    fi
}

restart_manual_daemon() {
    pkill -f "$DAEMON_PATH" 2>/dev/null || true
    sleep 1
    rm -f "$SOCKET_PATH"

    nohup "$DAEMON_PATH" </dev/null > "$STDOUT_PATH" 2> "$STDERR_PATH" &

    wait_count=0
    while [ "$wait_count" -lt 20 ]; do
        if [ -S "$SOCKET_PATH" ] || [ -e "$SOCKET_PATH" ]; then
            echo "✅ Build complete! voice_inputd started manually."
            return 0
        fi

        sleep 0.2
        wait_count=$((wait_count + 1))
    done

    echo "❌ voice_inputd did not become available after build." >&2
    echo "   stderr: $STDERR_PATH" >&2
    echo "   stdout: $STDOUT_PATH" >&2
    return 1
}

echo "🔨 Building voice_input..."
cd "$REPO_ROOT"
if ! cargo build --release; then
    echo "❌ Build failed" >&2
    exit 1
fi

echo "🔄 Restarting voice_inputd daemon..."
stop_launch_agent_if_loaded
restart_manual_daemon
