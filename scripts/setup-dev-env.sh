#!/bin/bash
# 開発環境セットアップスクリプト

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
        <key>DOTENV_PATH</key>
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
pkill -f voice_inputd 2>/dev/null
sleep 1
if launchctl kickstart -k user/$(id -u)/com.user.voiceinputd 2>/dev/null; then
    echo "Daemon restarted successfully."
else
    echo "Starting daemon manually..."
    nohup /usr/local/bin/voice_inputd_wrapper > /tmp/voice_inputd.out 2> /tmp/voice_inputd.err &
fi

echo ""
echo "✅ Setup complete!"
echo ""
echo "⚠️  重要: システム設定で権限を付与してください:"
echo ""
echo "1. システム設定を開く"
echo "2. プライバシーとセキュリティ → アクセシビリティ"
echo "3. 以下を追加して有効化:"
echo "   /usr/local/bin/voice_inputd_wrapper"
echo ""
echo "今後は以下のコマンドで開発できます:"
echo "  ./scripts/dev-build.sh"