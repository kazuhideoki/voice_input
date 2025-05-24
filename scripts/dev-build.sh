#!/bin/bash
# 開発用ビルドスクリプト（ラッパー使用版）

echo "🔨 Building voice_input..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "❌ Build failed"
    exit 1
fi

echo "🔄 Restarting voice_inputd daemon..."

# 既存のサービスを強制再起動
if launchctl kickstart -k user/$(id -u)/com.user.voiceinputd 2>/dev/null; then
    echo "✅ Build complete! voice_inputd has been restarted."
else
    echo "⚠️  kickstart failed, trying manual restart..."
    pkill -f voice_inputd 2>/dev/null
    sleep 1
    # 直接ラッパーを実行（バックグラウンド）
    nohup /usr/local/bin/voice_inputd_wrapper > /tmp/voice_inputd.out 2> /tmp/voice_inputd.err &
    echo "✅ Build complete! voice_inputd started manually."
fi