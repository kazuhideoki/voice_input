#!/bin/bash

DEPLOY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${VOICE_INPUT_REPO_ROOT:-$(cd "${DEPLOY_ROOT}/../.." && pwd)}"

LAUNCH_AGENT_LABEL="${VOICE_INPUT_LAUNCH_AGENT_LABEL:-com.user.voiceinputd}"
LAUNCH_AGENT_TARGET="gui/$(id -u)/${LAUNCH_AGENT_LABEL}"
LAUNCH_AGENT_PLIST_PATH="${VOICE_INPUT_LAUNCH_AGENT_PLIST_PATH:-$HOME/Library/LaunchAgents/${LAUNCH_AGENT_LABEL}.plist}"
SOCKET_PATH="${VOICE_INPUT_SOCKET_PATH:-/tmp/voice_input.sock}"
STDOUT_PATH="${VOICE_INPUT_STDOUT_PATH:-/tmp/voice_inputd.out}"
STDERR_PATH="${VOICE_INPUT_STDERR_PATH:-/tmp/voice_inputd.err}"
ENV_FILE_PATH="${VOICE_INPUT_ENV_FILE_PATH:-${REPO_ROOT}/.env}"
CODESIGN_BIN="${VOICE_INPUT_CODESIGN_BIN:-codesign}"
TCCUTIL_BIN="${VOICE_INPUT_TCCUTIL_BIN:-tccutil}"

load_profile() {
    DEPLOY_PROFILE="${1:-}"
    if [ -z "$DEPLOY_PROFILE" ]; then
        echo "❌ Missing deploy profile." >&2
        exit 1
    fi

    local profile_path="${DEPLOY_ROOT}/profiles/${DEPLOY_PROFILE}.sh"
    if [ ! -f "$profile_path" ]; then
        echo "❌ Unknown deploy profile: $DEPLOY_PROFILE" >&2
        exit 1
    fi

    # shellcheck disable=SC1090
    . "$profile_path"
}

launch_agent_loaded() {
    launchctl print "$LAUNCH_AGENT_TARGET" >/dev/null 2>&1
}

stop_launch_agent_if_loaded() {
    if ! launch_agent_loaded; then
        return 0
    fi

    echo "Stopping existing LaunchAgent before reconfiguration..."
    if ! launchctl bootout "$LAUNCH_AGENT_TARGET" 2>/dev/null; then
        echo "❌ Failed to stop LaunchAgent: $LAUNCH_AGENT_TARGET" >&2
        exit 1
    fi
}

remove_path_if_exists() {
    local path="$1"

    if [ ! -e "$path" ]; then
        return 0
    fi

    rm -rf "$path"
}

append_unique_path_entry() {
    local current_path="$1"
    local entry="$2"

    if [ -z "$entry" ]; then
        echo "$current_path"
        return 0
    fi

    case ":${current_path}:" in
        *":${entry}:"*) echo "$current_path" ;;
        *)
            if [ -n "$current_path" ]; then
                echo "${current_path}:${entry}"
            else
                echo "$entry"
            fi
            ;;
    esac
}

build_launch_agent_path() {
    echo "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
}

write_launch_agent_plist() {
    local launch_agent_path

    mkdir -p "$(dirname "$LAUNCH_AGENT_PLIST_PATH")"
    mkdir -p "$(dirname "$LAUNCH_PROGRAM_PATH")"
    launch_agent_path="$(build_launch_agent_path)"

    cat > "$LAUNCH_AGENT_PLIST_PATH" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${LAUNCH_AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${LAUNCH_PROGRAM_PATH}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>${STDERR_PATH}</string>
    <key>StandardOutPath</key>
    <string>${STDOUT_PATH}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>${launch_agent_path}</string>
        <key>HOME</key>
        <string>${HOME}</string>
        <key>VOICE_INPUT_ENV_PATH</key>
        <string>${ENV_FILE_PATH}</string>
    </dict>
    <key>WorkingDirectory</key>
    <string>${REPO_ROOT}</string>
</dict>
</plist>
PLIST
}

restart_launch_agent() {
    remove_path_if_exists "$SOCKET_PATH"

    if launch_agent_loaded; then
        if ! launchctl kickstart -k "$LAUNCH_AGENT_TARGET" 2>/dev/null; then
            echo "❌ Failed to kickstart LaunchAgent: $LAUNCH_AGENT_TARGET" >&2
            exit 1
        fi
        return 0
    fi

    if ! launchctl bootstrap "gui/$(id -u)" "$LAUNCH_AGENT_PLIST_PATH" 2>/dev/null; then
        echo "❌ Failed to bootstrap LaunchAgent: $LAUNCH_AGENT_PLIST_PATH" >&2
        exit 1
    fi
}

wait_for_socket() {
    local wait_count=0
    while [ "$wait_count" -lt 50 ]; do
        if [ -S "$SOCKET_PATH" ] || [ -e "$SOCKET_PATH" ]; then
            echo "✅ Build complete! ${PROFILE_SUCCESS_NAME} is available via LaunchAgent."
            return 0
        fi

        sleep 0.2
        wait_count=$((wait_count + 1))
    done

    echo "❌ ${PROFILE_SUCCESS_NAME} did not become available after build." >&2
    echo "   stderr: $STDERR_PATH" >&2
    echo "   stdout: $STDOUT_PATH" >&2
    exit 1
}

run_release_build() {
    echo "🔨 Building voice_input..."
    cd "$REPO_ROOT"
    cargo build --release
}

reset_tcc_permission() {
    local service="$1"
    local bundle_id="$2"

    if ! command -v "$TCCUTIL_BIN" >/dev/null 2>&1; then
        echo "ℹ️  Skipping TCC reset for ${service}: ${TCCUTIL_BIN} is not available."
        return 0
    fi

    "$TCCUTIL_BIN" reset "$service" "$bundle_id" >/dev/null 2>&1 || true
}
