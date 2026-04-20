#!/bin/bash

set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
. "${SCRIPT_DIR}/common.sh"

load_profile "${1:-}"

echo "🧹 Cleaning up ${DEPLOY_PROFILE} environment for voice_input..."

stop_launch_agent_if_loaded
remove_path_if_exists "$SOCKET_PATH"
remove_path_if_exists "$STDOUT_PATH"
remove_path_if_exists "$STDERR_PATH"
remove_path_if_exists "$LAUNCH_AGENT_PLIST_PATH"
cleanup_profile_artifacts
print_cleanup_summary
