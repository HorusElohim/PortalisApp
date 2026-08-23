#!/usr/bin/env bash
set -euo pipefail

# Build the Flutter application for one target platform.
#
# Examples:
#   tool/build.sh macos
#   tool/build.sh ios --clean --release
#   tool/build.sh android --debug
#   tool/build.sh web --dry-run

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RUST_DIR="$ROOT_DIR/rust/backend"

PLATFORM="macos"
CLEAN=0
DRY_RUN=0
AI_MODE=0
MODE="--debug"
FLUTTER_ARGS=()

usage() {
  cat <<'EOF'
Usage: tool/build.sh [platform] [options] [Flutter build options]

Platforms: ios, macos, android, linux, windows, web, chrome
Options:
  --clean    Remove Flutter and platform-native build artifacts first
  --debug    Build a debug application (default)
  --profile  Build a profile application
  --release  Build a release application
  --dry-run  Print the commands without executing them
  --ai       Minimize successful output; replay full output on error
  -h, --help Show this help
EOF
}

for argument in "$@"; do
  case "$argument" in
    ios|macos|android|linux|windows|web|chrome)
      PLATFORM="$argument"
      ;;
    --clean)
      CLEAN=1
      ;;
    --debug|--profile|--release)
      MODE="$argument"
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
      FLUTTER_ARGS+=("$argument")
      ;;
  esac
done

case "$PLATFORM" in
  chrome) FLUTTER_PLATFORM="web" ;;
  *) FLUTTER_PLATFORM="$PLATFORM" ;;
esac

run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    printf '+ '
    printf '%q ' "$@"
    printf '\n'
  elif [[ "$AI_MODE" -eq 1 ]]; then
    local log_file status
    log_file="$(mktemp "${TMPDIR:-/tmp}/portalis-build.XXXXXX")"
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

status() {
  echo "$@"
}

clean_platform() {
  status "==> Cleaning $PLATFORM artifacts"
  run flutter clean
  run rm -rf "$ROOT_DIR/build/$FLUTTER_PLATFORM"

  case "$FLUTTER_PLATFORM" in
    ios)
      run rm -rf "$ROOT_DIR/ios/Frameworks/backend.xcframework"
      run rm -rf "$RUST_DIR/target/aarch64-apple-ios"
      run rm -rf "$RUST_DIR/target/aarch64-apple-ios-sim"
      ;;
    android)
      run rm -rf "$ROOT_DIR/android/app/src/main/jniLibs"
      run rm -rf "$ROOT_DIR/android/app/.cxx"
      ;;
  esac
}

cd "$ROOT_DIR"

if [[ "$CLEAN" -eq 1 ]]; then
  clean_platform
fi

status "==> Resolving Dart packages"
run flutter pub get

status "==> Building $PLATFORM ($MODE)"
BUILD_ARGS=("$MODE")
if [[ "$FLUTTER_PLATFORM" == "ios" ]]; then
  BUILD_ARGS+=("--no-codesign")
fi
if [[ ${#FLUTTER_ARGS[@]} -gt 0 ]]; then
  BUILD_ARGS+=("${FLUTTER_ARGS[@]}")
fi

case "$FLUTTER_PLATFORM" in
  ios|macos|android|linux|windows|web)
    run flutter build "$FLUTTER_PLATFORM" "${BUILD_ARGS[@]}"
    ;;
  *)
    echo "Unsupported platform: $PLATFORM" >&2
    exit 2
    ;;
esac

echo "✅ Built $PLATFORM"
