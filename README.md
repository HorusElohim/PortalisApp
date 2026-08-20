# PortalisApp

**PortalisApp** is a peer-to-peer data-sharing app for private photo and video
collections. A Flutter client runs on top of a Rust backend; devices share media
directly over BitTorrent instead of uploading it through a central data service.

## Why PortalisApp

PortalisApp is the product, not a framework or starter template. Its first use
case is private, resilient sharing of personal media between devices and people
who already know each other. Collections remain useful when a central service is
unavailable: peers can exchange media directly on a local network or bootstrap a
connection from a QR code.

The app is designed around one source of truth per device: Rust owns identity,
collection state, persistence, peer discovery, and transfer orchestration;
Flutter renders that state and sends commands through the generated
Flutter-Rust bridge.

## What it does

- Create private collections from native media without copying files through
  Dart memory.
- Share collection data and media directly between peers over BitTorrent.
- Bootstrap nearby/offline transfers with QR magnet links carrying `x.pe` peer
  hints, with LAN discovery as an additional source of peers.
- Persist local identity, collection state, transfer intent, and media metadata
  in the device-owned backend.
- Report transfer progress and piece activity from the engine rather than
  inventing UI-side state.

## Architecture

```text
Flutter UI
    │  AppSnapshot / AppCommand
    ▼
Rust Nexus backend
    ├── collections, identity, persistence, and projection
    ├── peer discovery and BitTorrent substrate
    ├── Android/iOS platform adapters
    └── Flutter-Rust bridge
          │
          ▼
    Direct peer-to-peer media transfer
```

There is one native application backend: [`portalis/rust/backend/`](portalis/rust/backend/).
Its `nexus` namespace contains the application implementation; the crate root
contains only the app-facing API and generated bridge boundary.

Architecture decisions are recorded as frozen, append-only
[ADRs](doc/adr/README.md). The code is the current-state map.

## Repository layout

```text
portalis/                     Flutter application
├── lib/                      UI, feature state, and generated bridge adapters
├── rust/backend/             Nexus backend and protocol workspace
│   ├── src/nexus/            Application core and platform adapters
│   ├── crates/protocol/      Shared wire-format and validation contract
│   └── scripts/              Backend coverage tooling
├── android/ and ios/         Mobile platform integration
└── tool/                     Flutter-Rust bridge generation/build helpers

doc/adr/                      Frozen architecture decision records
tests/                        Reproducible backend and frontend validation
doc/                          Build, setup, and product-supporting guides
```

## Prerequisites

- Flutter SDK
- Rust toolchain via `rustup`
- [Buf](https://buf.build/) for protocol linting
- `cargo-llvm-cov` for backend coverage
- Android Studio with the Android SDK and NDK for Android builds
- Xcode for iOS and macOS builds

See the [setup guide](doc/setup_guide.md) and [build guide](doc/build.md) for
platform-specific requirements.

## Getting started

```sh
# Install Flutter dependencies.
cd portalis
flutter pub get

# From the repository root, validate the backend and frontend.
cd ..
./tests/all.sh
```

For day-to-day backend work:

```sh
cd portalis/rust/backend
buf lint
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```

## Testing and CI

- `./tests/nexus.sh` validates the Rust backend, protocol, coverage gate,
  demos, and release build.
- `./tests/frontend.sh` runs Flutter dependency resolution, analysis, and
  tests.
- `./tests/all.sh` runs both suites in the same sequence used by CI.

CI runs backend and frontend validation independently, then builds the
supported platform artifacts.

## Contributing

Keep changes focused and verify them before committing. Rust bridge schema
changes must include regenerated bindings; user-visible, runtime, or bridge
changes update `CHANGELOG.md`. See [`AGENTS.md`](AGENTS.md) for repository
conventions and [`doc/adr/README.md`](doc/adr/README.md) for architectural
context.
