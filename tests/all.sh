#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/.."

NEXUS_SCRIPT="$SCRIPT_DIR/nexus.sh"
FRONTEND_SCRIPT="$SCRIPT_DIR/frontend.sh"
IOS_RUST_SCRIPT="$SCRIPT_DIR/ios_rust_build_script.sh"
IOS_TLS_SCRIPT="$SCRIPT_DIR/ios_tls_provider.sh"

if [[ ! -x $NEXUS_SCRIPT ]]; then
  echo "[ERROR] Missing Nexus script at $NEXUS_SCRIPT" >&2
  exit 1
fi

if [[ ! -x $FRONTEND_SCRIPT ]]; then
  echo "[ERROR] Missing frontend script at $FRONTEND_SCRIPT" >&2
  exit 1
fi

if [[ ! -x $IOS_RUST_SCRIPT ]]; then
  echo "[ERROR] Missing iOS Rust build script test at $IOS_RUST_SCRIPT" >&2
  exit 1
fi

if [[ ! -x $IOS_TLS_SCRIPT ]]; then
  echo "[ERROR] Missing iOS TLS provider test at $IOS_TLS_SCRIPT" >&2
  exit 1
fi

pushd "$PROJECT_ROOT" >/dev/null

trap 'popd >/dev/null' EXIT

"$NEXUS_SCRIPT" "$@"
"$FRONTEND_SCRIPT" "$@"
"$IOS_RUST_SCRIPT"
"$IOS_TLS_SCRIPT"
