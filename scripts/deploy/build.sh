#!/bin/bash

set -eu

VOICE_INPUT_TMP="/tmp"
mkdir -p "$VOICE_INPUT_TMP"
export TMPDIR="$VOICE_INPUT_TMP"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
. "${SCRIPT_DIR}/common.sh"

load_profile "${1:-}"

run_release_build

echo "📦 Installing ${DEPLOY_PROFILE} artifacts..."
install_profile_artifacts
sign_profile_artifacts

echo "🔄 Restarting ${PROFILE_SUCCESS_NAME} via LaunchAgent..."
restart_launch_agent
wait_for_socket
