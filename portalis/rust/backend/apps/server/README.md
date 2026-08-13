# Portalis Nexus service

Nexus is Portalis's authenticated control plane. It carries identities,
contacts, encrypted-share metadata, key envelopes, and swarm discovery; media
travels directly between Portalis devices.

## Embedded deployment

The first supported deployment is one service with one persistent volume:

```sh
docker build -f apps/server/Dockerfile -t portalis-nexus .
docker run --read-only --tmpfs /tmp \
  --mount type=volume,src=portalis-nexus,dst=/var/lib/portalis-nexus \
  -p 8080:8080/tcp -p 8080:8080/udp \
  portalis-nexus
```

`PORTALIS_NEXUS_DATA_DIR` must survive restarts. It contains the embedded
database and the service's 32-byte `node-secret`; retaining both retains the
same public Node ID. The startup log prints that Node ID. Enter it, together
with the service's reachable UDP address, in Portalis Settings. Replacing the
secret creates a different authority and requires deliberate client
reconfiguration.

The same numeric port serves QUIC on UDP and probes on TCP:

```sh
curl http://localhost:8080/health/live
curl http://localhost:8080/health/ready
```

Use `PORTALIS_NEXUS_LISTEN_ADDR` to bind another address or port. Do not put
the node secret in logs, shell history, or an image layer; an operator may set
`PORTALIS_NEXUS_NODE_SECRET` instead of retaining the generated secret.

## Backup and restore

Stop the service cleanly, copy the complete persistent volume, then start it
again. Restore by stopping the replacement service and restoring that complete
copy before it starts. The database and `node-secret` are one backup unit: a
database restored without its original node secret no longer has the Node ID
that Portalis clients authenticate.
