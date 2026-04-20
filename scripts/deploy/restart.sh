#!/bin/bash

set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
. "${SCRIPT_DIR}/common.sh"

load_profile "${1:-}"

if [ ! -f "$LAUNCH_AGENT_PLIST_PATH" ]; then
    echo "❌ LaunchAgent plist was not found: $LAUNCH_AGENT_PLIST_PATH" >&2
    echo "   Run the corresponding setup script before restarting." >&2
    exit 1
fi

if declare -f validate_restart_prerequisites >/dev/null 2>&1; then
    validate_restart_prerequisites
fi

echo "🔄 Restarting ${PROFILE_SUCCESS_NAME} via LaunchAgent..."
restart_launch_agent
wait_for_socket
