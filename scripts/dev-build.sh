#!/bin/bash
# 開発用ビルドスクリプト

echo "🔨 Building voice_input..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "❌ Build failed"
    exit 1
fi

echo "🔏 Signing binaries..."
codesign -s - -f target/release/voice_input
codesign -s - -f target/release/voice_inputd

echo "🔄 Restarting voice_inputd daemon..."
launchctl unload ~/Library/LaunchAgents/com.user.voiceinputd.plist 2>/dev/null || true
launchctl load ~/Library/LaunchAgents/com.user.voiceinputd.plist

echo "✅ Build complete! voice_inputd has been restarted."