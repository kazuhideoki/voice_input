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
LAUNCH_AGENT_PLIST_PATH="${VOICE_INPUT_LAUNCH_AGENT_PLIST_PATH:-$HOME/Library/LaunchAgents/${LAUNCH_AGENT_LABEL}.plist}"
BUILD_DAEMON_PATH="${VOICE_INPUT_DAEMON_PATH:-${REPO_ROOT}/target/release/voice_inputd}"
INSTALLED_DAEMON_PATH="${VOICE_INPUT_INSTALLED_DAEMON_PATH:-$HOME/Library/Application Support/voice_input/bin/voice_inputd}"
SOCKET_PATH="${VOICE_INPUT_SOCKET_PATH:-/tmp/voice_input.sock}"
STDOUT_PATH="${VOICE_INPUT_STDOUT_PATH:-/tmp/voice_inputd.out}"
STDERR_PATH="${VOICE_INPUT_STDERR_PATH:-/tmp/voice_inputd.err}"
CODESIGN_BIN="${VOICE_INPUT_CODESIGN_BIN:-codesign}"
CODESIGN_IDENTIFIER="${VOICE_INPUT_CODESIGN_IDENTIFIER:-com.user.voiceinputd}"

launch_agent_loaded() {
    launchctl print "$LAUNCH_AGENT_TARGET" >/dev/null 2>&1
}

install_built_daemon() {
    if [ ! -x "$BUILD_DAEMON_PATH" ]; then
        echo "❌ Built daemon was not found: $BUILD_DAEMON_PATH" >&2
        exit 1
    fi

    mkdir -p "$(dirname "$INSTALLED_DAEMON_PATH")"
    cp "$BUILD_DAEMON_PATH" "$INSTALLED_DAEMON_PATH"
    chmod +x "$INSTALLED_DAEMON_PATH"
}

codesign_installed_daemon() {
    if ! "$CODESIGN_BIN" -f -s - --identifier "$CODESIGN_IDENTIFIER" "$INSTALLED_DAEMON_PATH"; then
        echo "❌ Failed to sign installed daemon: $INSTALLED_DAEMON_PATH" >&2
        exit 1
    fi
}

restart_launch_agent_daemon() {
    rm -f "$SOCKET_PATH"

    if launch_agent_loaded; then
        if ! launchctl kickstart -k "$LAUNCH_AGENT_TARGET" 2>/dev/null; then
            echo "❌ Failed to kickstart LaunchAgent: $LAUNCH_AGENT_TARGET" >&2
            exit 1
        fi
    else
        if ! launchctl bootstrap "gui/$(id -u)" "$LAUNCH_AGENT_PLIST_PATH" 2>/dev/null; then
            echo "❌ Failed to bootstrap LaunchAgent: $LAUNCH_AGENT_PLIST_PATH" >&2
            exit 1
        fi
    fi
}

wait_for_socket() {
    wait_count=0
    while [ "$wait_count" -lt 50 ]; do
        if [ -S "$SOCKET_PATH" ] || [ -e "$SOCKET_PATH" ]; then
            echo "✅ Build complete! voice_inputd is available via LaunchAgent."
            return 0
        fi

        sleep 0.2
        wait_count=$((wait_count + 1))
    done

    echo "❌ voice_inputd did not become available after build." >&2
    echo "   stderr: $STDERR_PATH" >&2
    echo "   stdout: $STDOUT_PATH" >&2
    exit 1
}

echo "🔨 Building voice_input..."
cd "$REPO_ROOT"
if ! cargo build --release; then
    echo "❌ Build failed" >&2
    exit 1
fi

echo "📦 Installing daemon for LaunchAgent..."
install_built_daemon
codesign_installed_daemon

echo "🔄 Restarting voice_inputd via LaunchAgent..."
restart_launch_agent_daemon
wait_for_socket
