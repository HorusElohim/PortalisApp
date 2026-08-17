#!/usr/bin/env bash
set -euo pipefail

# Builds and runs the Portalis app against whatever Nexus service is set up.
#
#   tool/run_app.sh               # macOS, debug
#   tool/run_app.sh ios           # or android, linux, windows, chrome…
#   tool/run_app.sh macos --release
#   tool/run_app.sh --codegen     # regenerate the Dart bridge first
#
# The Rust backend is compiled by the platform build (an Xcode phase on Apple
# platforms, Gradle on Android), so there is no separate step for it — running
# this after changing Rust picks the change up.
#
# --codegen is not the default because it is slow and only matters when the
# bridge API itself changed: a new or altered `pub` item under `src/api.rs` and
# what it reaches. Editing the inside of a function that already exists does
# not need it.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_DIR="$ROOT_DIR/.nexus-dev"
NODE_ID_FILE="$STATE_DIR/node_id"

DEVICE=""
CODEGEN=0
FLUTTER_ARGS=()

for argument in "$@"; do
  case "$argument" in
    --codegen) CODEGEN=1 ;;
    -*) FLUTTER_ARGS+=("$argument") ;;
    *)
      if [ -z "$DEVICE" ]; then
        DEVICE="$argument"
      else
        FLUTTER_ARGS+=("$argument")
      fi
      ;;
  esac
done
DEVICE="${DEVICE:-macos}"

cd "$ROOT_DIR"

if [ "$CODEGEN" -eq 1 ]; then
  echo "==> Regenerating the Dart bridge"
  tool/frb_build.sh
fi

echo "==> Resolving Dart packages"
flutter pub get

# The service this build defaults to, compiled in rather than pasted. A
# release pins the real one; here it is whichever service was last started
# from this checkout, so a debug build connects on its own exactly as a
# shipped one does — which is the behaviour worth testing.
#
# Settings still win: this is only what the app uses when nobody has chosen.
if [ -f "$NODE_ID_FILE" ]; then
  NODE_ID="$(cat "$NODE_ID_FILE")"
  PORTALIS_NEXUS_DEFAULT_NODE_ID="$NODE_ID"
  export PORTALIS_NEXUS_DEFAULT_NODE_ID
  # An address as well, because a container cannot answer mDNS and the app is
  # sandboxed, so relying on discovery alone here would test the wrong thing.
  PORTALIS_NEXUS_DEFAULT_ADDR="127.0.0.1:${PORTALIS_NEXUS_PORT:-8080}"
  export PORTALIS_NEXUS_DEFAULT_ADDR

  if command -v pbcopy >/dev/null 2>&1; then
    printf '%s' "$NODE_ID" | pbcopy
    CLIPBOARD="  (also on the clipboard)"
  else
    CLIPBOARD=""
  fi
  cat <<EOF

  Built against the service last started here, so the app connects on its
  own — nothing to paste.

    $NODE_ID$CLIPBOARD
    $PORTALIS_NEXUS_DEFAULT_ADDR

  Start it with tool/nexus_server.sh local if it is not running, or the app
  will sit at Not connected.

EOF
else
  cat <<EOF

  No Nexus service has been started from this checkout, so the app will run
  local-only — which is a valid thing to test, and the wrong thing to be
  surprised by.

    tool/nexus_server.sh local    quick, found without an address
    tool/nexus_server.sh          Docker and MongoDB, as deployed

EOF
fi

echo "==> Running on $DEVICE"
# Expanded through the `+` form because macOS ships bash 3.2, where an empty
# array under `set -u` is an unbound variable rather than nothing at all.
exec flutter run -d "$DEVICE" ${FLUTTER_ARGS[@]+"${FLUTTER_ARGS[@]}"}
