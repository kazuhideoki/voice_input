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

# 3. LaunchAgentのバックアップを作成
echo "Backing up LaunchAgent plist..."
cp ~/Library/LaunchAgents/com.user.voiceinputd.plist ~/Library/LaunchAgents/com.user.voiceinputd.plist.bak

# 4. LaunchAgentを更新
echo "Updating LaunchAgent to use wrapper..."
sed -i '' 's|/Users/kazuhideoki/voice_input/target/release/voice_inputd|/usr/local/bin/voice_inputd_wrapper|g' \
    ~/Library/LaunchAgents/com.user.voiceinputd.plist

# 5. デーモンを再起動
echo "Restarting daemon..."
launchctl unload ~/Library/LaunchAgents/com.user.voiceinputd.plist 2>/dev/null
launchctl load ~/Library/LaunchAgents/com.user.voiceinputd.plist

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
echo "今後は通常のビルドコマンドで開発できます:"
echo "  cargo build --release"
echo "  launchctl unload ~/Library/LaunchAgents/com.user.voiceinputd.plist && launchctl load ~/Library/LaunchAgents/com.user.voiceinputd.plist"