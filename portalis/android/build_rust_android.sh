#!/usr/bin/env bash
set -euo pipefail

# Build Rust shared libraries for Android ABIs and copy into jniLibs.

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CRATE_DIR="$ROOT_DIR/rust/backend"
APP_DIR="$ROOT_DIR/android/app"
JNILIBS_DIR="$APP_DIR/src/main/jniLibs"

# Detect build profile; default to release for smaller libs
BUILD_PROFILE="${1:-release}"
if [[ "$BUILD_PROFILE" != "debug" && "$BUILD_PROFILE" != "release" ]]; then
  BUILD_PROFILE=release
fi

mkdir -p "$JNILIBS_DIR"

STAMP="$JNILIBS_DIR/.portalis-rust-$BUILD_PROFILE.stamp"
ACTIVE_PROFILE="$JNILIBS_DIR/.portalis-rust-active-profile"
inputs_newer_than_stamp() {
  [[ ! -f "$STAMP" ]] && return 0
  for input in "$CRATE_DIR/Cargo.toml" "$CRATE_DIR/Cargo.lock" "$ROOT_DIR/android/build_rust_android.sh" "$CRATE_DIR/src" "$CRATE_DIR/vendor"; do
    if [[ -d "$input" ]]; then
      if find "$input" -type f -newer "$STAMP" -print -quit | grep -q .; then
        return 0
      fi
    elif [[ -f "$input" && "$input" -nt "$STAMP" ]]; then
      return 0
    fi
  done
  return 1
}

profile_changed=true
if [[ -f "$ACTIVE_PROFILE" ]] && [[ "$(<"$ACTIVE_PROFILE")" == "$BUILD_PROFILE" ]]; then
  profile_changed=false
fi

if ! inputs_newer_than_stamp && [[ "$profile_changed" == false ]]; then
  echo "==> Android Rust libraries are up to date ($BUILD_PROFILE); skipping cargo-ndk"
  exit 0
fi

if command -v cargo-ndk >/dev/null 2>&1; then
  echo "==> Using cargo-ndk to build Android libs ($BUILD_PROFILE)"
  # Debug and release must never coexist here: Gradle scans this directory for
  # every variant, so a release build after a debug build could otherwise
  # package the previous profile's native libraries when its own stamp looked
  # current. The active-profile marker above makes the next switch rebuild.
  rm -rf "$JNILIBS_DIR/arm64-v8a" "$JNILIBS_DIR/armeabi-v7a" "$JNILIBS_DIR/x86_64"
  pushd "$CRATE_DIR" >/dev/null
  if [[ "$BUILD_PROFILE" == "release" ]]; then
    cargo ndk -o "$JNILIBS_DIR" -t arm64-v8a -t x86_64 -t armeabi-v7a build --release
  else
    cargo ndk -o "$JNILIBS_DIR" -t arm64-v8a -t x86_64 -t armeabi-v7a build
  fi
  popd >/dev/null
else
  echo "(error) cargo-ndk not found. Install with: cargo install cargo-ndk" >&2
  echo "        Then run: rustup target add aarch64-linux-android x86_64-linux-android armv7-linux-androideabi" >&2
  exit 1
fi

touch "$STAMP"
printf '%s\n' "$BUILD_PROFILE" > "$ACTIVE_PROFILE"

echo "✅ JNI libs are in: $JNILIBS_DIR"

