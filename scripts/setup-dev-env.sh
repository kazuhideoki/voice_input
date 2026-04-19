#!/bin/bash
# 開発環境セットアップスクリプト

set -eu

REPO_ROOT="${VOICE_INPUT_REPO_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
LAUNCH_AGENT_LABEL="${VOICE_INPUT_LAUNCH_AGENT_LABEL:-com.user.voiceinputd}"
LAUNCH_AGENT_TARGET="gui/$(id -u)/${LAUNCH_AGENT_LABEL}"
LAUNCH_AGENT_PLIST_PATH="${VOICE_INPUT_LAUNCH_AGENT_PLIST_PATH:-$HOME/Library/LaunchAgents/${LAUNCH_AGENT_LABEL}.plist}"
INSTALLED_DAEMON_PATH="${VOICE_INPUT_INSTALLED_DAEMON_PATH:-$HOME/Library/Application Support/voice_input/bin/voice_inputd}"
SOCKET_PATH="${VOICE_INPUT_SOCKET_PATH:-/tmp/voice_input.sock}"
STDOUT_PATH="${VOICE_INPUT_STDOUT_PATH:-/tmp/voice_inputd.out}"
STDERR_PATH="${VOICE_INPUT_STDERR_PATH:-/tmp/voice_inputd.err}"
ENV_FILE_PATH="${VOICE_INPUT_ENV_FILE_PATH:-${REPO_ROOT}/.env}"

echo "📦 Setting up development environment for voice_input..."

launch_agent_loaded() {
    launchctl print "$LAUNCH_AGENT_TARGET" >/dev/null 2>&1
}

stop_launch_agent_if_loaded() {
    if ! launch_agent_loaded; then
        return 0
    fi

    echo "Stopping existing LaunchAgent before reconfiguration..."
    if ! launchctl bootout "$LAUNCH_AGENT_TARGET" 2>/dev/null; then
        echo "❌ Failed to stop LaunchAgent: $LAUNCH_AGENT_TARGET" >&2
        exit 1
    fi
}

write_launch_agent_plist() {
    mkdir -p "$(dirname "$LAUNCH_AGENT_PLIST_PATH")"
    mkdir -p "$(dirname "$INSTALLED_DAEMON_PATH")"

    cat > "$LAUNCH_AGENT_PLIST_PATH" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${LAUNCH_AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${INSTALLED_DAEMON_PATH}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>${STDERR_PATH}</string>
    <key>StandardOutPath</key>
    <string>${STDOUT_PATH}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
        <key>HOME</key>
        <string>${HOME}</string>
        <key>VOICE_INPUT_ENV_PATH</key>
        <string>${ENV_FILE_PATH}</string>
    </dict>
    <key>WorkingDirectory</key>
    <string>${REPO_ROOT}</string>
</dict>
</plist>
PLIST
}

stop_launch_agent_if_loaded
rm -f "$SOCKET_PATH"
rm -f "$STDOUT_PATH"
rm -f "$STDERR_PATH"
write_launch_agent_plist

echo ""
echo "✅ Setup complete!"
echo ""
echo "⚠️  重要: システム設定で権限を付与してください:"
echo ""
echo "1. システム設定を開く"
echo "2. プライバシーとセキュリティ → マイク"
echo "3. ${INSTALLED_DAEMON_PATH} を有効化"
echo "4. プライバシーとセキュリティ → アクセシビリティ"
echo "5. ${INSTALLED_DAEMON_PATH} を有効化"
echo ""
echo "今後は以下のコマンドで開発できます:"
echo "  ./scripts/dev-build.sh"
