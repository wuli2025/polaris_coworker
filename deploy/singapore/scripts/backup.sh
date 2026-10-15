#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
lock
prepare
require_docker
backup_authority
