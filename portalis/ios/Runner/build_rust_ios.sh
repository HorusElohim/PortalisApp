#!/usr/bin/env bash
set -euo pipefail

# Build Rust dynamic frameworks for iOS (device + simulator) and package as an XCFramework.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
IOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJ_DIR="$IOS_DIR"
RUST_DIR="$IOS_DIR/../rust/backend"
OUT_DIR="$IOS_DIR/Frameworks"

mkdir -p "$OUT_DIR"

# Ensure cargo is available when invoked from Xcode environment
export PATH="$HOME/.cargo/bin:$PATH"

# Cargo's built-in iOS triples default their linker floor to iOS 10.0, while
# Xcode builds the Runner for its IPHONEOS_DEPLOYMENT_TARGET. Exporting the
# latter keeps native dependency objects (cc-rs, aws-lc-sys and PhotoKit) and
# Rust's final linker invocation compatible with the same SDK floor.
IOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-15.0}"
if [[ ! $IOS_DEPLOYMENT_TARGET =~ ^[0-9]+(\.[0-9]+){0,2}$ ]]; then
  echo "(error) Invalid IPHONEOS_DEPLOYMENT_TARGET: $IOS_DEPLOYMENT_TARGET" >&2
  exit 1
fi
export IPHONEOS_DEPLOYMENT_TARGET="$IOS_DEPLOYMENT_TARGET"
export CARGO_TARGET_AARCH64_APPLE_IOS_RUSTFLAGS="${CARGO_TARGET_AARCH64_APPLE_IOS_RUSTFLAGS:-} -C link-arg=-miphoneos-version-min=${IOS_DEPLOYMENT_TARGET}"
export CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUSTFLAGS="${CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUSTFLAGS:-} -C link-arg=-mios-simulator-version-min=${IOS_DEPLOYMENT_TARGET}"

# Ensure required Rust targets (install if FRB_BOOTSTRAP=1)
function ensure_target() {
  local tgt="$1"
  if ! rustup target list --installed | grep -q "^${tgt}$"; then
    if [[ "${FRB_BOOTSTRAP:-0}" == "1" ]]; then
      echo "Installing Rust target ${tgt} ..."
      rustup target add "$tgt"
    else
      echo "(error) Missing Rust target: ${tgt}. Install with: rustup target add ${tgt}" >&2
      exit 1
    fi
  fi
}

ensure_target aarch64-apple-ios
ensure_target aarch64-apple-ios-sim || true

DEVICE_DYLIB="$RUST_DIR/target/aarch64-apple-ios/release/libbackend.dylib"
SIM_DYLIB_ARM64="$RUST_DIR/target/aarch64-apple-ios-sim/release/libbackend.dylib"

inputs_newer_than() {
  local target="$1" input
  [[ ! -f "$target" ]] && return 0
  for input in "$RUST_DIR/Cargo.toml" "$RUST_DIR/Cargo.lock" "$SCRIPT_DIR" "$RUST_DIR/src" "$RUST_DIR/vendor"; do
    if [[ -d "$input" ]]; then
      if find "$input" -type f -newer "$target" -print -quit | grep -q .; then
        return 0
      fi
    elif [[ -f "$input" && "$input" -nt "$target" ]]; then
      return 0
    fi
  done
  return 1
}

pushd "$RUST_DIR" >/dev/null
if inputs_newer_than "$DEVICE_DYLIB"; then
  cargo build --release --target aarch64-apple-ios
else
  echo "iOS device backend is up to date; skipping cargo build"
fi
if rustup target list --installed | grep -q '^aarch64-apple-ios-sim$'; then
  if inputs_newer_than "$SIM_DYLIB_ARM64"; then
    cargo build --release --target aarch64-apple-ios-sim
  else
    echo "iOS simulator backend is up to date; skipping cargo build"
  fi
fi
popd >/dev/null

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

create_framework() {
  local src_dylib="$1"; shift
  local dst_dir="$1"; shift
  local platform="$1"; shift  # iPhoneOS or iPhoneSimulator
  mkdir -p "$dst_dir"
  # Copy dylib and rename to framework binary name
  cp "$src_dylib" "$dst_dir/backend"
  # Ensure install_name uses @rpath so it is loadable from the embedded Frameworks directory
  if command -v install_name_tool >/dev/null 2>&1; then
    install_name_tool -id "@rpath/backend.framework/backend" "$dst_dir/backend" || true
  fi
  # Minimal Info.plist required by frameworks
  cat > "$dst_dir/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>backend</string>
  <key>CFBundleIdentifier</key>
  <string>com.portalis.backend</string>
  <key>CFBundleVersion</key>
  <string>1.0</string>
  <key>CFBundleShortVersionString</key>
  <string>1.0</string>
  <key>CFBundlePackageType</key>
  <string>FMWK</string>
  <key>CFBundleExecutable</key>
  <string>backend</string>
  <key>MinimumOSVersion</key>
  <string>${IOS_DEPLOYMENT_TARGET}</string>
</dict>
</plist>
PLIST
  # Add platform hint
  /usr/libexec/PlistBuddy -c "Add :CFBundleSupportedPlatforms array" "$dst_dir/Info.plist" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Add :CFBundleSupportedPlatforms:0 string $platform" "$dst_dir/Info.plist" 2>/dev/null || \
  /usr/libexec/PlistBuddy -c "Set :CFBundleSupportedPlatforms:0 $platform" "$dst_dir/Info.plist" 2>/dev/null || true
}

FWK_DEV="$TMP_DIR/Device/backend.framework"
FWK_SIM="$TMP_DIR/Simulator/backend.framework"
rm -rf "$FWK_DEV" "$FWK_SIM"

create_framework "$DEVICE_DYLIB" "$FWK_DEV" "iPhoneOS"
if [[ -f "$SIM_DYLIB_ARM64" ]]; then
  create_framework "$SIM_DYLIB_ARM64" "$FWK_SIM" "iPhoneSimulator"
fi

XC_OUT="$OUT_DIR/backend.xcframework"
if [[ -d "$XC_OUT" && ! "$DEVICE_DYLIB" -nt "$XC_OUT" && ! "$SIM_DYLIB_ARM64" -nt "$XC_OUT" && ! "$SCRIPT_DIR" -nt "$XC_OUT" ]]; then
  echo "iOS XCFramework is up to date; skipping packaging"
  exit 0
fi
rm -rf "$XC_OUT"

CMD=(xcodebuild -create-xcframework -framework "$FWK_DEV")
if [[ -d "$FWK_SIM" ]]; then
  CMD+=( -framework "$FWK_SIM" )
fi
CMD+=( -output "$XC_OUT" )
"${CMD[@]}"

echo "✅ Built XCFramework (framework-based) at: $XC_OUT"
