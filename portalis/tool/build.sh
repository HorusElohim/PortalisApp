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
source "$SCRIPT_DIR/parallelism.sh"

PLATFORM="macos"
CLEAN=0
DRY_RUN=0
AI_MODE=0
FORCE_FRB=0
NO_FRB=0
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
  --force-frb Force Flutter-Rust Bridge regeneration
  --no-frb    Skip Flutter-Rust Bridge generation
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
    --force-frb)
      FORCE_FRB=1
      ;;
    --no-frb)
      NO_FRB=1
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

# On native Windows shells, use the PowerShell pipeline so FRB generation,
# incremental Rust compilation, Flutter packaging, and backend.dll placement
# are one operation. Git Bash on Windows exposes either pwsh or powershell.exe.
if [[ "$PLATFORM" == "windows" ]]; then
  POWERSHELL=""
  if command -v pwsh >/dev/null 2>&1; then
    POWERSHELL="$(command -v pwsh)"
  elif command -v powershell.exe >/dev/null 2>&1; then
    POWERSHELL="$(command -v powershell.exe)"
  fi
  if [[ -n "$POWERSHELL" ]]; then
    PS_ARGS=("-NoProfile" "-ExecutionPolicy" "Bypass" "-File" "$SCRIPT_DIR/build_windows.ps1")
    case "$MODE" in
      --debug) PS_ARGS+=("-Configuration" "Debug") ;;
      --profile) PS_ARGS+=("-Configuration" "Profile") ;;
      --release) PS_ARGS+=("-Configuration" "Release") ;;
    esac
    [[ "$CLEAN" -eq 1 ]] && PS_ARGS+=("-Clean")
    [[ "$FORCE_FRB" -eq 1 ]] && PS_ARGS+=("-ForceFrb")
    [[ "$NO_FRB" -eq 1 ]] && PS_ARGS+=("-NoCodegen")
    run "$POWERSHELL" "${PS_ARGS[@]}"
    exit $?
  fi
fi

if [[ "$CLEAN" -eq 1 ]]; then
  clean_platform
fi

needs_pub_get() {
  local package_config="$ROOT_DIR/.dart_tool/package_config.json"
  local pub_stamp="$ROOT_DIR/.dart_tool/portalis/pub-get.stamp"
  [[ ! -f "$pub_stamp" ]] && return 0
  [[ ! -f "$package_config" ]] && return 0
  [[ "$ROOT_DIR/pubspec.yaml" -nt "$pub_stamp" ]] && return 0
  [[ "$ROOT_DIR/pubspec.lock" -nt "$pub_stamp" ]] && return 0
  return 1
}

if needs_pub_get; then
  status "==> Resolving Dart packages"
  run flutter pub get
  if [[ "$DRY_RUN" -eq 0 ]]; then
    mkdir -p "$ROOT_DIR/.dart_tool/portalis"
    touch "$ROOT_DIR/.dart_tool/portalis/pub-get.stamp"
  fi
else
  status "==> Dart packages are up to date"
fi

FRB_ARGS=("--codegen-only")
if [[ "$FORCE_FRB" -eq 1 ]]; then
  FRB_ARGS+=("--force-frb")
fi
if [[ "$NO_FRB" -eq 1 ]]; then
  FRB_ARGS+=("--no-frb")
fi
if [[ "$DRY_RUN" -eq 1 ]]; then
  FRB_ARGS+=("--dry-run")
fi
if [[ "$AI_MODE" -eq 1 ]]; then
  FRB_ARGS+=("--ai")
fi

status "==> Checking Flutter-Rust Bridge inputs"
run "$SCRIPT_DIR/frb_build.sh" "${FRB_ARGS[@]}"

# Xcode validates linked XCFramework paths while it plans Runner, before an
# aggregate target's build phase can materialize a missing framework. In
# particular, `--clean` deliberately removes this artifact, so recreate it
# before handing control to `flutter build ios`.
if [[ "$FLUTTER_PLATFORM" == "ios" ]]; then
  status "==> Building iOS Rust XCFramework"
  run bash "$ROOT_DIR/ios/Runner/build_rust_ios.sh"
fi

status "==> Building $PLATFORM ($MODE)"
BUILD_ARGS=("$MODE")
if [[ "$FLUTTER_PLATFORM" == "ios" ]]; then
  BUILD_ARGS+=("--no-codesign")
fi
if [[ ${#FLUTTER_ARGS[@]} -gt 0 ]]; then
  BUILD_ARGS+=("${FLUTTER_ARGS[@]}")
fi

case "$FLUTTER_PLATFORM" in
  android)
    # Rust produces only arm64-v8a now. Keep Flutter's packaging target in
    # lockstep so local builds do not request ABIs with no JNI library.
    has_target_platform=0
    has_project_cache_dir=0
    for argument in "${FLUTTER_ARGS[@]}"; do
      case "$argument" in
        --target-platform|--target-platform=*) has_target_platform=1 ;;
        --android-project-cache-dir|--android-project-cache-dir=*) has_project_cache_dir=1 ;;
      esac
    done
    if [[ "$has_target_platform" -eq 0 ]]; then
      BUILD_ARGS+=("--target-platform" "android-arm64")
    fi
    if [[ "$has_project_cache_dir" -eq 0 ]]; then
      BUILD_ARGS+=("--android-project-cache-dir" ".gradle/portalis-arm64")
    fi
    # Flutter has no `build android` command; APK is the canonical Android
    # artifact and matches the CI build action.
    run flutter build apk "${BUILD_ARGS[@]}"
    ;;
  ios|macos|linux|windows|web)
    run flutter build "$FLUTTER_PLATFORM" "${BUILD_ARGS[@]}"
    ;;
  *)
    echo "Unsupported platform: $PLATFORM" >&2
    exit 2
    ;;
esac

echo "✅ Built $PLATFORM"
