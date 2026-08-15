#!/usr/bin/env bash
set -euo pipefail

# Runs a Nexus service for testing an app against: the server and the MongoDB
# replica set it stores identities in, both under Docker Compose.
#
#   tool/nexus_server.sh          # bring it up and follow its log
#   tool/nexus_server.sh down     # stop it, keeping the data
#   tool/nexus_server.sh reset    # stop it and forget everything
#
# The node secret is generated once and kept in .nexus-dev/node_secret, so the
# service keeps the same Node ID across restarts — which is what makes it worth
# pasting into an app once rather than every time. The compose file requires
# the secret rather than defaulting it, because a service that mints a fresh
# identity on each start is one every device has to be told about again.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMPOSE_DIR="$ROOT_DIR/rust/backend/docker"
STATE_DIR="$ROOT_DIR/.nexus-dev"
SECRET_FILE="$STATE_DIR/node_secret"
LISTEN_PORT="${PORTALIS_NEXUS_PORT:-8080}"

if ! docker compose version >/dev/null 2>&1; then
  echo "[ERROR] docker compose not found on PATH" >&2
  exit 1
fi

mkdir -p "$STATE_DIR"
if [ ! -f "$SECRET_FILE" ]; then
  # 32 bytes, hex. The service's own identity, not a person's.
  openssl rand -hex 32 > "$SECRET_FILE"
  echo "==> Generated a service identity in $SECRET_FILE"
fi
PORTALIS_NEXUS_NODE_SECRET="$(cat "$SECRET_FILE")"
export PORTALIS_NEXUS_NODE_SECRET

cd "$COMPOSE_DIR"

case "${1:-up}" in
  down)
    docker compose down
    exit 0
    ;;
  reset)
    docker compose down --volumes
    rm -f "$SECRET_FILE"
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

cat <<EOF

  Portalis Nexus is listening.

    Node ID          $NODE_ID
    Direct address   127.0.0.1:$LISTEN_PORT
    Storage          MongoDB, in the compose volume

  Paste both into the app: Settings → Nexus service. The address is already
  the default there, so it is one paste of the Node ID.

  The identity is kept in $SECRET_FILE and survives a restart.

  Verify from here instead:

    cd rust/backend && PORTALIS_NEXUS_NODE_ID=$NODE_ID \\
      PORTALIS_NEXUS_ADDR=127.0.0.1:$LISTEN_PORT \\
      cargo test -p portalis-nexus-client --test connection \\
      reaches_a_separately_launched_server -- --ignored

  Devices arriving and leaving are logged. For every request as well, set
  RUST_LOG=info,portalis_nexus_server=debug in docker/compose.yaml.

  tool/nexus_server.sh down    stops it, keeping the data
  tool/nexus_server.sh reset   stops it and forgets everything

  Ctrl-C stops following the log; the service keeps running.

EOF

docker compose logs --follow nexus
