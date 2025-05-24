#!/bin/bash
# シンボリックリンクセットアップ

INSTALL_DIR="/usr/local/bin"
BINARY_NAME="voice_inputd"

echo "📦 Setting up symlink for $BINARY_NAME..."

# 既存のシンボリックリンクを削除
sudo rm -f "$INSTALL_DIR/$BINARY_NAME" 2>/dev/null

# 新しいシンボリックリンクを作成
sudo ln -s "/Users/kazuhideoki/voice_input/target/release/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"

echo "📝 Updating LaunchAgent plist..."
# plistを更新してシンボリックリンクを使うように変更
sed -i.bak "s|/Users/kazuhideoki/voice_input/target/release/voice_inputd|/usr/local/bin/voice_inputd|g" ~/Library/LaunchAgents/com.user.voiceinputd.plist

echo "✅ Setup complete!"
echo ""
echo "今後は以下のコマンドでビルドできます："
echo "./scripts/dev-build.sh"
echo ""
echo "⚠️  初回のみ、システム設定でアクセシビリティ権限を付与してください："
echo "   /usr/local/bin/voice_inputd"