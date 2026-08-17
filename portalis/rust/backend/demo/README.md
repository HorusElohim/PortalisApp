# Portalis Nexus demo

Runnable examples of the control plane. Nothing here is mocked: it is the real
server, the portable client, and real sockets between them.

## Guided walkthrough

```sh
cargo run -p portalis-nexus-demo
```

Starts a server on an ephemeral port and narrates one story end to end:
greeting, registration, two devices sharing a username, a replayed challenge
being refused, the same device authenticating on a fresh connection, an
unenrolled device being refused, and a correlated ping.

It then publishes an encrypted share, refuses a revision built on a snapshot
the share has moved past, shows a private share and a nonexistent one
answering identically, grants a second user access, and has her open the
sealed key and decrypt the capsule. It then uses the grant's returned device
record to send an encrypted `.torrent`/magnet descriptor to Grace's exact
live device; the server relays the ciphertext and the info hash but cannot
read either. The key is a real X25519 exchange: the server relays bytes it
cannot read. Finally two seeders announce to a swarm, discover each other at
the addresses their sockets were observed on, and one withdraws before its
lease expires. Then the server drains.

The demos enable compact structured client and server transport logs on
stderr. Their numbered walkthrough remains on stdout, so it can be redirected
independently. `RUST_LOG` overrides the default filters.

## Two processes

Start the server:

```sh
cargo run -p portalis-nexus-server
```

Then run the client. The first run registers; every later run authenticates
with the key it saved:

```sh
cargo run -p portalis-nexus-demo --bin client
cargo run -p portalis-nexus-demo --bin client
```

Arguments are `[endpoint] [username]`, and `PORTALIS_NEXUS_DEMO_KEY` chooses
where the device keys are written (`demo-device.key` by default). That file
holds two 32-byte secrets side by side: the Ed25519 signing seed and the
X25519 encryption secret. Delete it to start over as a new device.

The server keeps identities in memory unless `PORTALIS_NEXUS_MONGODB_URI` is
set, so without it a restart forgets them. With it, the same key authenticates
across restarts:

```sh
PORTALIS_NEXUS_MONGODB_URI='mongodb://localhost:27017/?directConnection=true' \
  cargo run -p portalis-nexus-server
```

MongoDB must be a replica set: registration writes a user and its first device
in one transaction. `docker/compose.yaml` starts one.

## What to read

- `src/lib.rs` — `DemoDevice` implements `DeviceSigner`. This is the seam an
  application fills: the client only ever sees the public key and finished
  signatures, never the private key, so a real app can back it with a keychain
  or secure enclave.
- `src/bin/client.rs` — authenticate first, register only when the server does
  not know the device. Note that each connection may sign once, so the attempt
  spends that connection's challenge either way.
- `src/main.rs` — the same calls, with a server embedded in the process.

## Server identity

A signature is bound to the Nexus server's authenticated QUIC Node ID. The
client takes that ID from the `EndpointAddr` it dials; Iroh verifies the peer
owns the corresponding private key during the handshake. The address is only
a routing hint and never enters a signed payload.

For a separately started server, keep `PORTALIS_NEXUS_NODE_SECRET` stable and
use the public Node ID it logs at startup when constructing the client's
`EndpointAddr`. The same identity works through a direct address, relay, DNS
name, or container network change without invalidating device signatures.
