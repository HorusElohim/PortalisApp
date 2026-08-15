#!/usr/bin/env bash
set -euo pipefail

# Runs a Nexus service for testing an app against.
#
# Embedded storage, so there is no MongoDB to install, and the node secret is
# kept beside the data — the service keeps the same identity across restarts,
# which is what makes the values printed below worth pasting into an app once
# rather than every time.
#
#   tool/nexus_server.sh              # ./.nexus-dev, 127.0.0.1:4433
#   tool/nexus_server.sh 0.0.0.0:4433 # reachable from a phone on the LAN
#   PORTALIS_NEXUS_DATA_DIR=/tmp/x tool/nexus_server.sh

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND_DIR="$ROOT_DIR/rust/backend"

LISTEN="${1:-127.0.0.1:4433}"
DATA_DIR="${PORTALIS_NEXUS_DATA_DIR:-$ROOT_DIR/.nexus-dev}"

mkdir -p "$DATA_DIR"
cd "$BACKEND_DIR"

echo "==> Building the service"
cargo build -p portalis-nexus-server

BINARY="$BACKEND_DIR/target/debug/portalis-nexus-server"

# The server logs JSON, which is right for a deployment and unreadable when
# what you need is two values to type into Settings. So the identity is read
# out of the store before starting, and printed plainly.
#
# Starting once to learn the identity and again to serve would be two
# processes; instead this starts it, waits for the ready line, and prints the
# values from that line.
LOG="$(mktemp -t nexus-server)"
PORTALIS_NEXUS_DATA_DIR="$DATA_DIR" \
PORTALIS_NEXUS_LISTEN_ADDR="$LISTEN" \
  "$BINARY" > "$LOG" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true; rm -f "$LOG"' EXIT INT TERM

for _ in $(seq 1 100); do
  if grep -q '"Portalis Nexus is ready"' "$LOG" 2>/dev/null; then break; fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "[ERROR] the service exited before it was ready:" >&2
    cat "$LOG" >&2
    exit 1
  fi
  sleep 0.1
done

NODE_ID="$(grep -o '"node_id":"[a-f0-9]*"' "$LOG" | head -1 | cut -d'"' -f4)"
if [ -z "$NODE_ID" ]; then
  echo "[ERROR] the service never reported a node id:" >&2
  cat "$LOG" >&2
  exit 1
fi

cat <<EOF

  Portalis Nexus is listening.

    Node ID          $NODE_ID
    Direct address   $LISTEN
    Storage          embedded, $DATA_DIR

  Paste both into the app: Settings → Network & engine → Nexus service.
  The identity is kept in $DATA_DIR, so it survives a restart.

  Verify from here instead:

    cd rust/backend && PORTALIS_NEXUS_NODE_ID=$NODE_ID \\
      PORTALIS_NEXUS_ADDR=$LISTEN \\
      cargo test -p portalis-nexus-client --test connection \\
      reaches_a_separately_launched_server -- --ignored

  Ctrl-C to stop.

EOF

# Follow the service's own log from here, so a failure after startup is
# visible rather than buried in a temporary file.
tail -f "$LOG" &
wait "$SERVER_PID"
