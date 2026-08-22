#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILDER="$SCRIPT_DIR/../portalis/ios/Runner/build_rust_ios.sh"

if [[ ! -f $BUILDER ]]; then
  echo "[ERROR] Missing iOS Rust builder at $BUILDER" >&2
  exit 1
fi

require() {
  local expected="$1"
  if ! grep -Fq -- "$expected" "$BUILDER"; then
    echo "[ERROR] iOS Rust builder is missing: $expected" >&2
    exit 1
  fi
}

# Native build scripts (cc-rs, aws-lc-sys, PhotoKit adapter) must compile for
# the same floor as Xcode's target rather than the SDK's newest version.
require 'IOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-15.0}"'
require 'export IPHONEOS_DEPLOYMENT_TARGET="$IOS_DEPLOYMENT_TARGET"'

# Rust's built-in aarch64-apple-ios target defaults to iOS 10.0. Target-local
# flags must lift the final device/simulator linker invocation to Xcode's floor.
require 'CARGO_TARGET_AARCH64_APPLE_IOS_RUSTFLAGS='
require 'CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUSTFLAGS='
require '-miphoneos-version-min=${IOS_DEPLOYMENT_TARGET}'
require '-mios-simulator-version-min=${IOS_DEPLOYMENT_TARGET}'

# The packaged framework must not claim a lower OS than the binary was built for.
require '<string>${IOS_DEPLOYMENT_TARGET}</string>'

echo "iOS Rust deployment-target propagation is configured"
