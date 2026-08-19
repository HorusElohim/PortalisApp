#![cfg_attr(not(frb_expand), allow(unexpected_cfgs))]
/// Android-only bridge used by Rust-owned MediaStore storage.
#[cfg(target_os = "android")]
mod android_content;
mod api; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */
/// The Flutter-facing version handshake.
pub mod bridge;
/// The one collection model the app renders — see its module doc. Replaces
/// the former `collab` module, which was one half of a pair that was never
/// joined.
pub mod collections;
/// Canonical storage locations; never cache-path fallbacks.
mod content_location;
/// The running core: state, workers, and the command boundary.
pub mod core;
/// Iroh-free crypto and revision verification, used by collection publish
/// and receive workflows. Extracted from the former Iroh-based Nexus client
/// crate when its transport was removed.
mod crypto;
pub mod device;
/// This device's signing identity.
mod domain;
/// Durable no-copy gallery source descriptors for seeding after restart.
mod linked_source_store;
mod log;
/// Where persisted state lives — one place, so tests can move it.
mod paths;
/// The single app-facing Nexus lifecycle, streams, and command boundary.
pub mod portalis_api;
pub mod projection;
/// Every knob librqbit exposes, persisted and bridged — see its module doc.
pub mod settings;
pub mod store;
/// What moves the bytes — see docs/torrent-engine.md.
mod substrate;
/// The librqbit engine behind `substrate::Torrents` — see its module doc.
/// Its DTOs stay compiled on every target (wasm32 included) because
/// `substrate` is target-agnostic; only the implementation is gated,
/// falling back to an error on wasm32.
pub mod torrent;
/// One JSON document, read whole or written whole.
mod vault;
