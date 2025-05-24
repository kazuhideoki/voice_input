#!/bin/bash
# 開発用ビルドスクリプト v2

echo "🔨 Building voice_input..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "❌ Build failed"
    exit 1
fi

echo "🔏 Signing binaries..."
codesign -s - -f target/release/voice_input
codesign -s - -f target/release/voice_inputd

echo "🔄 Stopping voice_inputd daemon..."
launchctl unload ~/Library/LaunchAgents/com.user.voiceinputd.plist 2>/dev/null || true

# 古いプロセスが完全に終了するまで待つ
sleep 1

echo "📋 Clearing TCC cache (requires sudo)..."
# TCCキャッシュをクリアして新しいバイナリを認識させる
sudo killall tccd 2>/dev/null || true

echo "🚀 Starting voice_inputd daemon..."
launchctl load ~/Library/LaunchAgents/com.user.voiceinputd.plist

echo "✅ Build complete!"
echo ""
echo "⚠️  権限設定が必要な場合："
echo "1. システム設定 → プライバシーとセキュリティ → アクセシビリティ"
echo "2. 以下を確認/追加："
echo "   - 使用中のターミナル"
echo "   - /Users/kazuhideoki/voice_input/target/release/voice_inputd"
echo ""
echo "💡 ヒント: 権限ダイアログが表示されたら「許可」をクリック"