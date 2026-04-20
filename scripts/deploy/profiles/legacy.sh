#!/bin/bash

BUILD_DAEMON_PATH="${VOICE_INPUT_DAEMON_PATH:-${REPO_ROOT}/target/release/voice_inputd}"
INSTALLED_DAEMON_PATH="${VOICE_INPUT_INSTALLED_DAEMON_PATH:-$HOME/Library/Application Support/voice_input/bin/voice_inputd}"
LEGACY_CODESIGN_IDENTIFIER="${VOICE_INPUT_CODESIGN_IDENTIFIER:-com.user.voiceinputd}"

LAUNCH_PROGRAM_PATH="$INSTALLED_DAEMON_PATH"
PROFILE_SUCCESS_NAME="voice_inputd"

prepare_profile_layout() {
    mkdir -p "$(dirname "$INSTALLED_DAEMON_PATH")"
}

install_profile_artifacts() {
    if [ ! -x "$BUILD_DAEMON_PATH" ]; then
        echo "❌ Built daemon was not found: $BUILD_DAEMON_PATH" >&2
        exit 1
    fi

    mkdir -p "$(dirname "$INSTALLED_DAEMON_PATH")"
    cp "$BUILD_DAEMON_PATH" "$INSTALLED_DAEMON_PATH"
    chmod +x "$INSTALLED_DAEMON_PATH"
}

sign_profile_artifacts() {
    if ! "$CODESIGN_BIN" -f -s - --identifier "$LEGACY_CODESIGN_IDENTIFIER" "$INSTALLED_DAEMON_PATH"; then
        echo "❌ Failed to sign installed daemon: $INSTALLED_DAEMON_PATH" >&2
        exit 1
    fi
}

cleanup_profile_artifacts() {
    remove_path_if_exists "$INSTALLED_DAEMON_PATH"
}

print_setup_next_steps() {
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
}

print_cleanup_summary() {
    echo ""
    echo "✅ Cleanup complete!"
    echo ""
    echo "ℹ️  注意: macOS のマイク/アクセシビリティ権限は自動削除されません。"
    echo "   システム設定 → プライバシーとセキュリティから手動で整理してください。"
}
