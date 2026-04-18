#!/bin/bash
# 開発用ビルドスクリプト（ラッパー使用版）

set -u

# Rustc が macOS 15 の一部環境で root 所有の /var/folders/zz/.../T を参照して
# Permission denied になる問題への暫定対応として、書き込み可能な専用 TMPDIR を設定する。
VOICE_INPUT_TMP="/tmp"
if ! mkdir -p "$VOICE_INPUT_TMP"; then
    echo "❌ TMPDIR の作成に失敗しました: $VOICE_INPUT_TMP" >&2
    exit 1
fi
export TMPDIR="$VOICE_INPUT_TMP"

LAUNCH_AGENT_LABEL="com.user.voiceinputd"
LAUNCH_AGENT_TARGET="gui/$(id -u)/${LAUNCH_AGENT_LABEL}"
WRAPPER_PATH="/usr/local/bin/voice_inputd_wrapper"
DAEMON_PATH="$(pwd)/target/release/voice_inputd"
SOCKET_PATH="/tmp/voice_input.sock"

launch_agent_loaded() {
    launchctl print "$LAUNCH_AGENT_TARGET" >/dev/null 2>&1
}

restart_loaded_launch_agent() {
    if launchctl kickstart -k "$LAUNCH_AGENT_TARGET" 2>/dev/null; then
        echo "✅ Build complete! voice_inputd has been restarted via LaunchAgent."
        return 0
    fi

    echo "❌ LaunchAgent restart failed." >&2
    echo "   Refusing manual start to avoid duplicate voice_inputd processes." >&2
    echo "   Resolve the LaunchAgent state first: launchctl bootout $LAUNCH_AGENT_TARGET" >&2
    return 1
}

restart_manual_daemon() {
    pkill -f "$DAEMON_PATH" 2>/dev/null || true
    sleep 1
    rm -f "$SOCKET_PATH"

    nohup "$WRAPPER_PATH" > /tmp/voice_inputd.out 2> /tmp/voice_inputd.err &
    echo "✅ Build complete! voice_inputd started manually."
}

echo "🔨 Building voice_input..."
if ! cargo build --release; then
    echo "❌ Build failed"
    exit 1
fi


echo "🔄 Restarting voice_inputd daemon..."

# LaunchAgent 利用中は manual fallback で二重起動させない
if launch_agent_loaded; then
    restart_loaded_launch_agent || exit 1
else
    echo "ℹ️  LaunchAgent is not loaded, using manual restart path..."
    restart_manual_daemon
fi
