#!/usr/bin/env bash
set -euo pipefail

# Conditional Flutter-Rust Bridge generation and optional native backend build.
#
# Code generation is driven by a content fingerprint. It runs only when a
# bridge input or generated output changed, unless --force-frb is supplied.
# Native platform builds remain explicit so Flutter/Xcode/Gradle can own their
# platform packaging hooks without this helper rebuilding them unnecessarily.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CRATE="$ROOT_DIR/rust/backend"
STAMP_DIR="$ROOT_DIR/.dart_tool/portalis"
FRB_STAMP="$STAMP_DIR/frb-inputs.sha256"

PLATFORM="macos"
CODEGEN_ONLY=0
FORCE_FRB=0
NO_FRB=0
DRY_RUN=0
AI_MODE=0

usage() {
  cat <<'EOF'
Usage: tool/frb_build.sh [platform] [options]

Platforms: macos, ios, android, linux, windows, web, all
Options:
  --codegen-only  Check/regenerate FRB bindings without a native build
  --force-frb     Regenerate FRB even when the input fingerprint is unchanged
  --no-frb        Skip FRB generation entirely
  --dry-run       Print commands without executing them
  --ai            Minimize successful output; replay full output on error
  -h, --help      Show this help
EOF
}

for argument in "$@"; do
  case "$argument" in
    macos|ios|android|linux|windows|web|all)
      PLATFORM="$argument"
      ;;
    --codegen-only)
      CODEGEN_ONLY=1
      ;;
    --force-frb)
      FORCE_FRB=1
      ;;
    --no-frb)
      NO_FRB=1
      ;;
    --dry-run)
      DRY_RUN=1
      ;;
    --ai)
      AI_MODE=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $argument" >&2
      usage >&2
      exit 2
      ;;
  esac
done

run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    printf '+ '
    printf '%q ' "$@"
    printf '\n'
  elif [[ "$AI_MODE" -eq 1 ]]; then
    local log_file status
    log_file="$(mktemp "${TMPDIR:-/tmp}/portalis-frb.XXXXXX")"
    if "$@" >"$log_file" 2>&1; then
      rm -f "$log_file"
    else
      status=$?
      cat "$log_file" >&2
      rm -f "$log_file"
      return "$status"
    fi
  else
    "$@"
  fi
}

hash_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 | awk '{print $1}'
  else
    echo "(error) sha256sum or shasum is required for incremental FRB generation" >&2
    return 1
  fi
}

hash_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

generator_version() {
  if command -v flutter_rust_bridge_codegen >/dev/null 2>&1; then
    flutter_rust_bridge_codegen --version 2>/dev/null || printf 'unknown'
  else
    printf 'missing'
  fi
}

# These are the explicit FRB boundary modules. Backend-only changes outside
# this boundary do not force a costly regeneration.
FRB_INPUTS=(
  "$CRATE/src/bridge.rs"
  "$CRATE/src/portalis_api.rs"
  "$CRATE/src/nexus/device.rs"
  "$CRATE/src/nexus/settings.rs"
  "$CRATE/Cargo.toml"
  "$CRATE/Cargo.lock"
  "$ROOT_DIR/pubspec.yaml"
  "$SCRIPT_DIR/frb_build.sh"
)

FRB_OUTPUTS=(
  "$CRATE/src/api.rs"
  "$ROOT_DIR/lib/nexus/bridge/bridge.dart"
  "$ROOT_DIR/lib/nexus/bridge/portalis_api.dart"
  "$ROOT_DIR/lib/nexus/bridge/frb_generated.dart"
  "$ROOT_DIR/lib/nexus/bridge/frb_generated.io.dart"
  "$ROOT_DIR/lib/nexus/bridge/frb_generated.web.dart"
  "$ROOT_DIR/lib/nexus/bridge/nexus/device.dart"
  "$ROOT_DIR/lib/nexus/bridge/nexus/settings.dart"
)

fingerprint() {
  {
    printf 'generator=%s\n' "$(generator_version)"
    for input in "${FRB_INPUTS[@]}"; do
      if [[ -f "$input" ]]; then
        printf '%s=%s\n' "${input#"$ROOT_DIR/"}" "$(hash_file "$input")"
      else
        printf '%s=MISSING\n' "${input#"$ROOT_DIR/"}"
      fi
    done
  } | hash_stream
}

outputs_present() {
  local output
  for output in "${FRB_OUTPUTS[@]}"; do
    [[ -f "$output" ]] || return 1
  done
}

codegen_needed() {
  [[ "$FORCE_FRB" -eq 1 ]] && return 0
  outputs_present || return 0
  [[ -f "$FRB_STAMP" ]] || return 0
  [[ "$(cat "$FRB_STAMP")" == "$(fingerprint)" ]] || return 0
  return 1
}

resolve_codegen_tool() {
  if ! command -v flutter_rust_bridge_codegen >/dev/null 2>&1 \
      && [[ -n "${USERPROFILE:-}" ]] \
      && command -v cygpath >/dev/null 2>&1; then
    local cargo_bin
    cargo_bin=$(cygpath -u "${CARGO_HOME:-$USERPROFILE/.cargo}/bin")
    export PATH="$PATH:$cargo_bin"
  fi
  command -v flutter_rust_bridge_codegen >/dev/null 2>&1 || {
    echo "(error) flutter_rust_bridge_codegen is required because FRB outputs are missing or stale." >&2
    echo "        Install with: cargo install flutter_rust_bridge_codegen" >&2
    return 1
  }
}

maybe_codegen() {
  if [[ "$NO_FRB" -eq 1 ]]; then
    echo "==> Skipping FRB generation (--no-frb)"
    return 0
  fi

  local current
  current="$(fingerprint)"
  if ! codegen_needed; then
    echo "==> FRB bindings are up to date"
    return 0
  fi

  resolve_codegen_tool
  echo "==> Regenerating flutter_rust_bridge bindings"
  run flutter_rust_bridge_codegen generate \
    --rust-root "$CRATE" \
    --rust-input crate::bridge,crate::portalis_api,crate::nexus::device,crate::nexus::settings \
    --dart-output "lib/nexus/bridge" \
    --rust-output "$CRATE/src/api.rs" \
    --no-add-mod-to-lib
  run cargo fmt --manifest-path "$CRATE/Cargo.toml" --all

  if [[ "$DRY_RUN" -eq 0 ]]; then
    mkdir -p "$STAMP_DIR"
    fingerprint >"$FRB_STAMP"
  else
    printf 'Would write FRB stamp %s=%s\n' "$FRB_STAMP" "$current"
  fi
}

build_macos() {
  echo "==> cargo build (macOS)"
  run cargo build --manifest-path "$CRATE/Cargo.toml" --release
  echo "Built: $CRATE/target/release/libbackend.dylib"
}

build_linux() {
  echo "==> cargo build (Linux)"
  run cargo build --manifest-path "$CRATE/Cargo.toml" --release
  echo "Built: $CRATE/target/release/libbackend.so"
}

build_windows() {
  echo "==> cargo build (Windows)"
  run cargo build --manifest-path "$CRATE/Cargo.toml" --release
  echo "Built: $CRATE/target/release/backend.dll"
}

build_web() {
  echo "==> Web backend build"
  run cargo build --manifest-path "$CRATE/Cargo.toml" --release --target wasm32-unknown-unknown
}

build_android() {
  echo "==> Android backend build via cargo-ndk"
  run bash "$ROOT_DIR/android/build_rust_android.sh" release
}

build_ios() {
  echo "==> iOS backend build"
  run bash "$ROOT_DIR/ios/Runner/build_rust_ios.sh"
}

cd "$ROOT_DIR"
maybe_codegen
[[ "$CODEGEN_ONLY" -eq 1 ]] && exit 0

case "$PLATFORM" in
  macos) build_macos ;;
  ios) build_ios ;;
  android) build_android ;;
  linux) build_linux ;;
  windows) build_windows ;;
  web) build_web ;;
  all)
    build_macos
    build_ios
    build_android
    build_linux
    build_windows
    build_web
    ;;
  *) echo "Unknown platform: $PLATFORM" >&2; exit 2 ;;
esac

echo "✅ Done: $PLATFORM"
