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
APP_BUNDLE_CODESIGN_REQUIREMENTS="${VOICE_INPUT_APP_BUNDLE_CODESIGN_REQUIREMENTS:-designated => identifier \"${APP_BUNDLE_IDENTIFIER}\"}"

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
    if ! "$CODESIGN_BIN" -f -s - --deep --identifier "$APP_BUNDLE_IDENTIFIER" "-r=${APP_BUNDLE_CODESIGN_REQUIREMENTS}" "$APP_BUNDLE_PATH"; then
        echo "❌ Failed to sign app bundle: $APP_BUNDLE_PATH" >&2
        exit 1
    fi
}

cleanup_profile_artifacts() {
    remove_path_if_exists "$APP_BUNDLE_PATH"
    reset_tcc_permission "Microphone" "$APP_BUNDLE_IDENTIFIER"
    reset_tcc_permission "Accessibility" "$APP_BUNDLE_IDENTIFIER"
}

validate_restart_prerequisites() {
    if [ ! -x "$BUNDLED_DAEMON_PATH" ]; then
        echo "❌ Bundled daemon was not found: $BUNDLED_DAEMON_PATH" >&2
        echo "   Run ./scripts/build-app-bundle.sh before restarting the app bundle." >&2
        exit 1
    fi
}

print_setup_next_steps() {
    echo ""
    echo "✅ Setup complete!"
    echo ""
    echo "初回は以下の順序で app bundle を利用してください:"
    echo "  1. ./scripts/build-app-bundle.sh"
    echo "  2. システム設定で VoiceInput.app にマイク/アクセシビリティ権限を付与"
    echo "  3. ./scripts/restart-app-bundle.sh"
    echo ""
    echo "app bundle を更新する場合は以下のコマンドを利用できます:"
    echo "  ./scripts/build-app-bundle.sh"
    echo "権限付与の反映だけであれば以下で十分です:"
    echo "  ./scripts/restart-app-bundle.sh"
}

print_cleanup_summary() {
    echo ""
    echo "✅ Cleanup complete!"
    echo ""
    echo "ℹ️  VoiceInput.app を削除し、bundle identifier に紐づく TCC 設定を reset しました。"
}
