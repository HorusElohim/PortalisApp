# PortalisApp Nexus backend

This crate is the native backend for **PortalisApp**, a peer-to-peer data-sharing
application for private photo and video collections. It is not a reusable server
framework and it has no separate control-plane service: the application owns its
local state and peers exchange media directly through BitTorrent.

Rust owns the device-local source of truth:

- device identity and collection membership;
- persisted collection, import, and transfer state;
- BitTorrent descriptor resolution, seeding, and acquisition;
- QR peer hints and LAN peer discovery;
- Android and iOS native media adapters;
- the narrow Flutter-facing `AppSnapshot` / `AppCommand` boundary.

Flutter renders that backend state and sends commands through
`flutter_rust_bridge`; it does not own a parallel copy of collection or transfer
truth.

## Layout

```text
src/
├── api.rs                 Generated Flutter-Rust bridge glue
├── bridge.rs              App compatibility/version boundary
├── portalis_api.rs        App-facing snapshot and command mapping
└── nexus/                 Internal application implementation
    ├── collections/       Collection model, publishing, receiving, membership
    ├── core/              Lifecycle, commands, workers, and transfer state
    ├── crypto/            Key sealing and revision verification
    ├── platform/          Android JNI and iOS PhotoKit adapters
    ├── projection/        State emitted to Flutter
    ├── store/             Device-owned redb persistence
    ├── substrate.rs       Peer discovery and BitTorrent abstraction
    └── torrent.rs         librqbit adapter and media I/O

crates/protocol/           Shared canonical formats and validation
scripts/coverage.sh        Backend coverage gate
```

The `nexus` namespace is deliberately internal. The crate root exposes only the
app-facing API and generated bridge modules.

## Architecture decisions

PortalisApp records architecture as frozen, append-only
[ADRs](../../../doc/adr/README.md). The code is the current-state map. Do not add
a replacement central specification or migration plan; introduce a new ADR when
a decision changes the architecture.

## Development

Run backend checks from this directory:

```sh
buf lint
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
./scripts/coverage.sh
```

From the repository root, `./tests/nexus.sh` runs the complete backend gate:
formatting, linting, tests, coverage, demos, and release build.

## Flutter bridge

When a public Rust bridge signature or DTO changes, regenerate the bindings from
`portalis/` with the repository helper for the target platform:

```sh
./tool/frb_build.sh <macos|ios|android|linux|windows|web>
```

Keep generated Dart bindings and the native library in the same commit. Internal
Nexus refactors that do not change a bridged signature do not require
regeneration.

## Platform builds

Android builds require the Android SDK and NDK. iOS and macOS builds require
Xcode. The platform hooks build the Rust library for mobile targets; see the
repository [build guide](../../../doc/build.md) for exact commands.
