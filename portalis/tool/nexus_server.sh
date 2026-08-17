#!/usr/bin/env bash
set -euo pipefail

# Runs a Nexus service for testing an app against.
#
#   tool/nexus_server.sh          # Docker Compose, on MongoDB — what deploys
#   tool/nexus_server.sh local    # the binary, on the embedded store — quick
#   tool/nexus_server.sh down     # stop the compose stack, keeping the data
#   tool/nexus_server.sh reset    # stop it and forget everything
#
# Both modes read the same node secret, so they answer to the same Node ID.
# Switching between them does not invalidate what an app already has pasted in,
# and only one can hold the port at a time, which is the intended arrangement.
#
# The secret is generated once and kept in .nexus-dev/node_secret, because a
# service that mints a fresh identity on each start is one every device has to
# be told about again. The compose file requires it rather than defaulting it
# for the same reason.
#
# Which to use: `local` builds in seconds and stores identities in a directory,
# which is what app QA wants. The default is Docker on MongoDB, which is what
# actually deploys — use it before believing anything about storage.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_DIR="$ROOT_DIR/rust/backend/docker"
BACKEND_DIR="$ROOT_DIR/rust/backend"
STATE_DIR="$ROOT_DIR/.nexus-dev"
SECRET_FILE="$STATE_DIR/node_secret"
NODE_ID_FILE="$STATE_DIR/node_id"
LOCAL_STORE="$STATE_DIR/local-store"
LISTEN_PORT="${PORTALIS_NEXUS_PORT:-8080}"

mkdir -p "$STATE_DIR"
if [ ! -f "$SECRET_FILE" ]; then
  # 32 bytes, hex. The service's own identity, not a person's.
  openssl rand -hex 32 > "$SECRET_FILE"
  echo "==> Generated a service identity in $SECRET_FILE"
fi
PORTALIS_NEXUS_NODE_SECRET="$(cat "$SECRET_FILE")"
export PORTALIS_NEXUS_NODE_SECRET

# Printed once the Node ID is known, whichever mode found it out.
announce() {
  local node_id="$1" storage="$2"
  printf '%s' "$node_id" > "$NODE_ID_FILE"
  cat <<EOF

  Portalis Nexus is listening.

    Node ID          $node_id
    Direct address   127.0.0.1:$LISTEN_PORT
    Storage          $storage

  Paste the Node ID into the app: Settings → Nexus service. That alone is
  enough — the app resolves it over this network. An address is optional, and
  only skips the lookup.

  tool/run_app.sh puts this Node ID on the clipboard and starts the app.

  Verify from here instead:

    cd rust/backend && PORTALIS_NEXUS_NODE_ID=$node_id \\
      cargo test -p portalis-nexus-client --test connection \\
      reaches_a_separately_launched_server -- --ignored

  Devices arriving and leaving are logged.

EOF
}

MODE="${1:-up}"

case "$MODE" in
  local)
    # No Mongo, no image: registrations land in a directory. Same identity as
    # the compose stack, so an app configured against one reaches the other.
    # Checked before building, because the failure otherwise arrives a minute
    # later as "Address already in use" from a log, and the usual culprit is
    # the compose stack still holding the port this mode wants.
    if lsof -nP -iUDP:"$LISTEN_PORT" >/dev/null 2>&1; then
      echo "[ERROR] something is already listening on UDP $LISTEN_PORT." >&2
      echo "        If that is the compose stack: tool/nexus_server.sh down" >&2
      echo "        Or pick another: PORTALIS_NEXUS_PORT=8099 $0 local" >&2
      exit 1
    fi

    mkdir -p "$LOCAL_STORE"
    echo "==> Building the service"
    cargo build --quiet --manifest-path "$BACKEND_DIR/Cargo.toml" -p portalis-nexus-server

    BINARY="$BACKEND_DIR/target/debug/portalis-nexus-server"
    LOG_FILE="$STATE_DIR/local.log"
    PORTALIS_NEXUS_LISTEN_ADDR="0.0.0.0:$LISTEN_PORT" \
    PORTALIS_NEXUS_DATA_DIR="$LOCAL_STORE" \
    PORTALIS_NEXUS_LOG=text \
    RUST_LOG="${RUST_LOG:-portalis_nexus_server=info}" \
      "$BINARY" > "$LOG_FILE" 2>&1 &
    SERVICE_PID=$!
    # Stop the service when this script is interrupted, rather than leaving it
    # holding the port for the next run to fail on.
    trap 'kill "$SERVICE_PID" 2>/dev/null || true' EXIT INT TERM

    echo "==> Waiting for the service"
    NODE_ID=""
    for _ in $(seq 1 60); do
      NODE_ID="$(grep -oE 'node_id=[a-f0-9]{64}' "$LOG_FILE" 2>/dev/null \
        | tail -1 | cut -d= -f2 || true)"
      [ -n "$NODE_ID" ] && break
      if ! kill -0 "$SERVICE_PID" 2>/dev/null; then
        echo "[ERROR] the service stopped before it was ready:" >&2
        cat "$LOG_FILE" >&2
        exit 1
      fi
      sleep 1
    done

    if [ -z "$NODE_ID" ]; then
      echo "[ERROR] the service never reported a node id:" >&2
      cat "$LOG_FILE" >&2
      exit 1
    fi

    announce "$NODE_ID" "a directory, at $LOCAL_STORE"
    echo "  Ctrl-C stops the service. Identities are kept; delete that"
    echo "  directory to meet every device as a stranger again."
    echo
    tail -f "$LOG_FILE"
    exit 0
    ;;
esac

if ! docker compose version >/dev/null 2>&1; then
  echo "[ERROR] docker compose not found on PATH" >&2
  echo "        tool/nexus_server.sh local runs the service without it" >&2
  exit 1
fi

cd "$COMPOSE_DIR"

case "$MODE" in
  down)
    docker compose down
    exit 0
    ;;
  reset)
    docker compose down --volumes
    rm -f "$SECRET_FILE" "$NODE_ID_FILE"
    rm -rf "$LOCAL_STORE"
    echo "==> Stopped, and forgot both the stored identities and this service's own."
    exit 0
    ;;
esac

echo "==> Building and starting Nexus and MongoDB"
docker compose up --build --detach

# Ready is when the service says so, not when the container starts: it waits
# for Mongo to become primary first, and dialling before then fails in a way
# that looks like a wrong address.
echo "==> Waiting for the service"
NODE_ID=""
for _ in $(seq 1 180); do
  NODE_ID="$(docker compose logs nexus 2>/dev/null \
    | grep -oE 'node_id=[a-f0-9]{64}' | tail -1 | cut -d= -f2 || true)"
  [ -n "$NODE_ID" ] && break
  if [ -z "$(docker compose ps --quiet nexus)" ]; then
    echo "[ERROR] the service container is gone:" >&2
    docker compose logs nexus >&2
    exit 1
  fi
  sleep 1
done

if [ -z "$NODE_ID" ]; then
  echo "[ERROR] the service never reported a node id:" >&2
  docker compose logs nexus >&2
  exit 1
fi

announce "$NODE_ID" "MongoDB, in the compose volume"
cat <<EOF
  A container cannot answer mDNS across Docker's network, so with this mode
  set the direct address too. tool/nexus_server.sh local can be found without
  one.

  For every request as well, set RUST_LOG=info,portalis_nexus_server=debug in
  docker/compose.yaml.

  tool/nexus_server.sh down    stops it, keeping the data
  tool/nexus_server.sh reset   stops it and forgets everything

  Ctrl-C stops following the log; the service keeps running.

EOF

docker compose logs --follow nexus
