#!/bin/bash
# 開発環境セットアップスクリプト

set -eu

REPO_ROOT="${VOICE_INPUT_REPO_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
LAUNCH_AGENT_LABEL="${VOICE_INPUT_LAUNCH_AGENT_LABEL:-com.user.voiceinputd}"
LAUNCH_AGENT_TARGET="gui/$(id -u)/${LAUNCH_AGENT_LABEL}"
SOCKET_PATH="${VOICE_INPUT_SOCKET_PATH:-/tmp/voice_input.sock}"
STDOUT_PATH="${VOICE_INPUT_STDOUT_PATH:-/tmp/voice_inputd.out}"
STDERR_PATH="${VOICE_INPUT_STDERR_PATH:-/tmp/voice_inputd.err}"
DAEMON_PATH="${VOICE_INPUT_DAEMON_PATH:-${REPO_ROOT}/target/release/voice_inputd}"

echo "📦 Setting up development environment for voice_input..."

launch_agent_loaded() {
    launchctl print "$LAUNCH_AGENT_TARGET" >/dev/null 2>&1
}

stop_launch_agent_if_loaded() {
    if ! launch_agent_loaded; then
        return 0
    fi

    echo "Stopping LaunchAgent to use terminal-managed daemon..."
    if ! launchctl bootout "$LAUNCH_AGENT_TARGET" 2>/dev/null; then
        echo "❌ Failed to stop LaunchAgent: $LAUNCH_AGENT_TARGET" >&2
        exit 1
    fi
}

stop_existing_daemon() {
    if pkill -f "$DAEMON_PATH" 2>/dev/null; then
        echo "Stopped existing daemon: $DAEMON_PATH"
    fi
}

stop_launch_agent_if_loaded
stop_existing_daemon
rm -f "$SOCKET_PATH"
rm -f "$STDOUT_PATH"
rm -f "$STDERR_PATH"

echo ""
echo "✅ Setup complete!"
echo ""
echo "⚠️  重要: システム設定で権限を付与してください:"
echo ""
echo "1. システム設定を開く"
echo "2. プライバシーとセキュリティ → マイク"
echo "3. 使用中のターミナルを有効化"
echo "4. プライバシーとセキュリティ → アクセシビリティ"
echo "5. 使用中のターミナルを有効化"
echo ""
echo "今後は以下のコマンドで開発できます:"
echo "  ./scripts/dev-build.sh"
