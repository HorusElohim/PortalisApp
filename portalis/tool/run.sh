#!/usr/bin/env bash
set -euo pipefail

# Build and run Portalis on one target platform.
#
# Examples:
#   tool/run.sh macos
#   tool/run.sh ios --clean --device 00008110-...
#   tool/run.sh android --release
#   tool/run.sh web --dry-run

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

PLATFORM="macos"
DEVICE=""
CLEAN=0
DRY_RUN=0
AI_MODE=0
MODE="--debug"
FLUTTER_ARGS=()

usage() {
  cat <<'EOF'
Usage: tool/run.sh [platform] [options] [Flutter run options]

Platforms: ios, macos, android, linux, windows, web, chrome
Options:
  --clean           Clean platform artifacts before building
  --debug           Build and run debug (default)
  --profile         Build and run profile
  --release         Build and run release
  --device <id>     Explicit Flutter device ID; useful for iOS/Android
  --dry-run         Print build/run commands without executing them
  --ai              Minimize successful output; replay full output on error
  -h, --help        Show this help
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
    --device)
      # The next argument is consumed below by the indexed parser instead.
      ;;
    -h|--help)
      usage
      exit 0
      ;;
  esac
done

# Parse again so --device can consume its value without relying on Bash 4-only
# associative arrays; macOS still ships Bash 3.2.
POSITIONAL=()
EXPECT_DEVICE=0
for argument in "$@"; do
  if [[ "$EXPECT_DEVICE" -eq 1 ]]; then
    DEVICE="$argument"
    EXPECT_DEVICE=0
    continue
  fi
  if [[ "$argument" == "--device" ]]; then
    EXPECT_DEVICE=1
    continue
  fi
  case "$argument" in
    ios|macos|android|linux|windows|web|chrome|--clean|--debug|--profile|--release|--dry-run|--ai)
      ;;
    *)
      POSITIONAL+=("$argument")
      ;;
  esac
done
if [[ "$EXPECT_DEVICE" -eq 1 ]]; then
  echo "--device requires a Flutter device ID" >&2
  exit 2
fi
FLUTTER_ARGS=("${POSITIONAL[@]}")

run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    printf '+ '
    printf '%q ' "$@"
    printf '\n'
  elif [[ "$AI_MODE" -eq 1 ]]; then
    local log_file status
    log_file="$(mktemp "${TMPDIR:-/tmp}/portalis-run.XXXXXX")"
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

cd "$ROOT_DIR"

BUILD_ARGS=("$PLATFORM" "$MODE")
if [[ "$CLEAN" -eq 1 ]]; then
  BUILD_ARGS+=("--clean")
fi
if [[ "$DRY_RUN" -eq 1 ]]; then
  BUILD_ARGS+=("--dry-run")
fi
if [[ "$AI_MODE" -eq 1 ]]; then
  BUILD_ARGS+=("--ai")
fi
BUILD_ARGS+=("${FLUTTER_ARGS[@]}")

# build.sh performs dependency resolution and the platform build first.
echo "==> Building $PLATFORM ($MODE)"
run "$SCRIPT_DIR/build.sh" "${BUILD_ARGS[@]}"

echo "==> Running on ${DEVICE:-$PLATFORM}"
RUN_ARGS=("$MODE")
case "$PLATFORM" in
  ios)
    # Without --device Flutter selects the available iOS device/simulator.
    ;;
  chrome|web)
    RUN_ARGS+=("-d" "chrome")
    ;;
  macos|android|linux|windows)
    RUN_ARGS+=("-d" "$PLATFORM")
    ;;
  *)
    echo "Unsupported platform: $PLATFORM" >&2
    exit 2
    ;;
esac
if [[ -n "$DEVICE" ]]; then
  RUN_ARGS+=("-d" "$DEVICE")
fi
RUN_ARGS+=("${FLUTTER_ARGS[@]}")

if [[ "$DRY_RUN" -eq 1 ]]; then
  run flutter run "${RUN_ARGS[@]}"
elif [[ "$AI_MODE" -eq 1 ]]; then
  run flutter run "${RUN_ARGS[@]}"
  echo "✅ Run completed successfully"
else
  exec flutter run "${RUN_ARGS[@]}"
fi
