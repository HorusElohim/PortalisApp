#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

PORTALIS_DIR="$ROOT_DIR/portalis"
source "$PORTALIS_DIR/tool/parallelism.sh"
if [[ ! -d $PORTALIS_DIR ]]; then
  echo "[ERROR] Portalis project not found at $PORTALIS_DIR" >&2
  exit 1
fi

pushd "$PORTALIS_DIR" >/dev/null

if ! command -v flutter >/dev/null 2>&1; then
  echo "[ERROR] flutter not found on PATH" >&2
  exit 1
fi

echo "==> flutter pub get"
flutter pub get

echo "==> flutter analyze"
flutter analyze

echo "==> flutter test --no-pub ${*:+${*}}"
flutter test --no-pub --concurrency "$PORTALIS_FLUTTER_TEST_CONCURRENCY" "$@"

echo "==> FRB generated drift check"
./tool/frb_build.sh --codegen-only --force-frb --ai
if ! git diff --quiet -- \
  rust/backend/src/api.rs \
  lib/nexus/bridge/bridge.dart \
  lib/nexus/bridge/portalis_api.dart \
  lib/nexus/bridge/frb_generated.dart \
  lib/nexus/bridge/frb_generated.io.dart \
  lib/nexus/bridge/frb_generated.web.dart \
  lib/nexus/bridge/nexus/device.dart \
  lib/nexus/bridge/nexus/settings.dart; then
  echo "[ERROR] generated FRB output is stale; run ./tool/frb_build.sh --codegen-only" >&2
  git diff --stat -- \
    rust/backend/src/api.rs \
    lib/nexus/bridge
  exit 1
fi

popd >/dev/null
