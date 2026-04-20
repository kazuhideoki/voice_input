#!/bin/bash

set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
. "${SCRIPT_DIR}/common.sh"

load_profile "${1:-}"

echo "📦 Setting up ${DEPLOY_PROFILE} environment for voice_input..."

prepare_profile_layout
stop_launch_agent_if_loaded
remove_path_if_exists "$SOCKET_PATH"
remove_path_if_exists "$STDOUT_PATH"
remove_path_if_exists "$STDERR_PATH"
write_launch_agent_plist
print_setup_next_steps
