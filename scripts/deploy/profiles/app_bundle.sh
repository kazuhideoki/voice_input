#!/bin/bash

BUILD_CLI_PATH="${VOICE_INPUT_BUILD_CLI_PATH:-${REPO_ROOT}/target/release/voice_input}"
BUILD_DAEMON_PATH="${VOICE_INPUT_DAEMON_PATH:-${REPO_ROOT}/target/release/voice_inputd}"
APP_BUNDLE_PATH="${VOICE_INPUT_APP_BUNDLE_PATH:-$HOME/Applications/VoiceInput.app}"
APP_BUNDLE_CONTENTS_PATH="${VOICE_INPUT_APP_BUNDLE_CONTENTS_PATH:-${APP_BUNDLE_PATH}/Contents}"
APP_BUNDLE_MACOS_PATH="${VOICE_INPUT_APP_BUNDLE_MACOS_PATH:-${APP_BUNDLE_CONTENTS_PATH}/MacOS}"
APP_BUNDLE_INFO_PLIST_PATH="${VOICE_INPUT_APP_BUNDLE_INFO_PLIST_PATH:-${APP_BUNDLE_CONTENTS_PATH}/Info.plist}"
BUNDLED_CLI_PATH="${VOICE_INPUT_BUNDLED_CLI_PATH:-${APP_BUNDLE_MACOS_PATH}/voice_input}"
BUNDLED_DAEMON_PATH="${VOICE_INPUT_BUNDLED_DAEMON_PATH:-${APP_BUNDLE_MACOS_PATH}/voice_inputd}"
APP_BUNDLE_IDENTIFIER="${VOICE_INPUT_APP_BUNDLE_IDENTIFIER:-com.user.voiceinput}"

LAUNCH_PROGRAM_PATH="$BUNDLED_DAEMON_PATH"
PROFILE_SUCCESS_NAME="VoiceInput.app"

prepare_profile_layout() {
    mkdir -p "$APP_BUNDLE_MACOS_PATH"
}

write_info_plist() {
    mkdir -p "$(dirname "$APP_BUNDLE_INFO_PLIST_PATH")"

    cat > "$APP_BUNDLE_INFO_PLIST_PATH" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>voice_inputd</string>
    <key>CFBundleIdentifier</key>
    <string>${APP_BUNDLE_IDENTIFIER}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>VoiceInput</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>VoiceInput records microphone audio for speech transcription.</string>
</dict>
</plist>
PLIST
}

install_profile_artifacts() {
    if [ ! -x "$BUILD_CLI_PATH" ]; then
        echo "❌ Built CLI was not found: $BUILD_CLI_PATH" >&2
        exit 1
    fi

    if [ ! -x "$BUILD_DAEMON_PATH" ]; then
        echo "❌ Built daemon was not found: $BUILD_DAEMON_PATH" >&2
        exit 1
    fi

    rm -rf "$APP_BUNDLE_PATH"
    mkdir -p "$APP_BUNDLE_MACOS_PATH"
    cp "$BUILD_CLI_PATH" "$BUNDLED_CLI_PATH"
    cp "$BUILD_DAEMON_PATH" "$BUNDLED_DAEMON_PATH"
    chmod +x "$BUNDLED_CLI_PATH" "$BUNDLED_DAEMON_PATH"
    write_info_plist
}

sign_profile_artifacts() {
    if ! "$CODESIGN_BIN" -f -s - --deep --identifier "$APP_BUNDLE_IDENTIFIER" "$APP_BUNDLE_PATH"; then
        echo "❌ Failed to sign app bundle: $APP_BUNDLE_PATH" >&2
        exit 1
    fi
}

cleanup_profile_artifacts() {
    remove_path_if_exists "$APP_BUNDLE_PATH"
    reset_tcc_permission "Microphone" "$APP_BUNDLE_IDENTIFIER"
    reset_tcc_permission "Accessibility" "$APP_BUNDLE_IDENTIFIER"
}

print_setup_next_steps() {
    echo ""
    echo "✅ Setup complete!"
    echo ""
    echo "ℹ️  初回起動時に VoiceInput.app へマイク/アクセシビリティ権限を付与してください。"
    echo "今後は以下のコマンドで app bundle を更新できます:"
    echo "  ./scripts/build-app-bundle.sh"
}

print_cleanup_summary() {
    echo ""
    echo "✅ Cleanup complete!"
    echo ""
    echo "ℹ️  VoiceInput.app を削除し、bundle identifier に紐づく TCC 設定を reset しました。"
}
