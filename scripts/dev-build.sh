#!/bin/bash
# 開発用ビルドスクリプト（ラッパー使用版）

# Rustc が macOS 15 の一部環境で root 所有の /var/folders/zz/.../T を参照して
# Permission denied になる問題への暫定対応として、書き込み可能な専用 TMPDIR を設定する。
VOICE_INPUT_TMP="/tmp"
if ! mkdir -p "$VOICE_INPUT_TMP"; then
    echo "❌ TMPDIR の作成に失敗しました: $VOICE_INPUT_TMP" >&2
    exit 1
fi
export TMPDIR="$VOICE_INPUT_TMP"

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

