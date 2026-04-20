#!/bin/bash

set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
/bin/bash "${SCRIPT_DIR}/deploy/cleanup.sh" legacy
