#!/bin/bash
# 開発環境セットアップスクリプト

set -u

LAUNCH_AGENT_LABEL="com.user.voiceinputd"
LAUNCH_AGENT_TARGET="gui/$(id -u)/${LAUNCH_AGENT_LABEL}"
WRAPPER_PATH="/usr/local/bin/voice_inputd_wrapper"

echo "📦 Setting up development environment for voice_input..."

# 1. ラッパースクリプトを作成
echo "Creating wrapper script..."
cat > /tmp/voice_inputd_wrapper << 'EOF'
#!/bin/bash
exec /Users/kazuhideoki/voice_input/target/release/voice_inputd "$@"
EOF

# 2. 適切な場所に配置
echo "Installing wrapper script (requires sudo)..."
sudo mv /tmp/voice_inputd_wrapper /usr/local/bin/
sudo chmod +x /usr/local/bin/voice_inputd_wrapper

# 3. LaunchAgentのバックアップを作成（存在する場合）
if [ -f ~/Library/LaunchAgents/com.user.voiceinputd.plist ]; then
    echo "Backing up LaunchAgent plist..."
    cp ~/Library/LaunchAgents/com.user.voiceinputd.plist ~/Library/LaunchAgents/com.user.voiceinputd.plist.bak
    
    # 4. LaunchAgentを更新
    echo "Updating LaunchAgent to use wrapper..."
    sed -i '' 's|/Users/kazuhideoki/voice_input/target/release/voice_inputd|/usr/local/bin/voice_inputd_wrapper|g' \
        ~/Library/LaunchAgents/com.user.voiceinputd.plist
else
    # 4. LaunchAgentを新規作成
    echo "Creating LaunchAgent plist..."
    cat > ~/Library/LaunchAgents/com.user.voiceinputd.plist << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.user.voiceinputd</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/voice_inputd_wrapper</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>/tmp/voice_inputd.err</string>
    <key>StandardOutPath</key>
    <string>/tmp/voice_inputd.out</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
        <key>HOME</key>
        <string>/Users/kazuhideoki</string>
        <key>VOICE_INPUT_ENV_PATH</key>
        <string>/Users/kazuhideoki/voice_input/.env</string>
    </dict>
    <key>WorkingDirectory</key>
    <string>/Users/kazuhideoki/voice_input</string>
</dict>
</plist>
PLIST
fi

# 5. デーモンを再起動
echo "Restarting daemon..."
if launchctl print "$LAUNCH_AGENT_TARGET" >/dev/null 2>&1; then
    launchctl bootout "$LAUNCH_AGENT_TARGET" 2>/dev/null || true
fi
pkill -f "/Users/kazuhideoki/voice_input/target/release/voice_inputd" 2>/dev/null || true
sleep 1
rm -f /tmp/voice_input.sock
if launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/${LAUNCH_AGENT_LABEL}.plist" 2>/dev/null; then
    echo "Daemon loaded successfully."
elif launchctl kickstart -k "$LAUNCH_AGENT_TARGET" 2>/dev/null; then
    echo "Daemon restarted successfully."
else
    echo "❌ Failed to load LaunchAgent." >&2
    echo "   Resolve the LaunchAgent state first instead of starting manually." >&2
    echo "   launchctl bootout $LAUNCH_AGENT_TARGET" >&2
    exit 1
fi

echo ""
echo "✅ Setup complete!"
echo ""
echo "⚠️  重要: システム設定で権限を付与してください:"
echo ""
echo "1. システム設定を開く"
echo "2. プライバシーとセキュリティ → マイク"
echo "3. 以下を追加して有効化:"
echo "   - 使用中のターミナル"
echo "   - $WRAPPER_PATH"
echo "4. プライバシーとセキュリティ → アクセシビリティ"
echo "5. 以下を追加して有効化:"
echo "   - 使用中のターミナル"
echo "   - $WRAPPER_PATH"
echo ""
echo "今後は以下のコマンドで開発できます:"
echo "  ./scripts/dev-build.sh"
