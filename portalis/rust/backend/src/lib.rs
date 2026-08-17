#![cfg_attr(not(frb_expand), allow(unexpected_cfgs))]
/// Android-only bridge used by Rust-owned MediaStore storage.
#[cfg(target_os = "android")]
mod android_content;
mod api; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */
/// The Flutter-facing version handshake.
pub mod bridge;
/// Canonical storage locations; never cache-path fallbacks.
mod content_location;
/// The running core: state, workers, and the command boundary.
pub mod core;
/// This device's Nexus identity.
mod domain;
/// Durable no-copy gallery source descriptors for seeding after restart.
mod linked_source_store;
mod log;
/// The signing identity every publication is authored by.
mod nexus;
/// The trusted Nexus service address persisted for the app connection.
pub mod nexus_settings;
/// Where persisted state lives — one place, so tests can move it.
mod paths;
/// The single app-facing Nexus lifecycle, streams, and command boundary.
pub mod portalis_api;
pub mod projection;
pub mod store;
/// What moves the bytes — see docs/future-engine.md.
mod substrate;
/// One JSON document, read whole or written whole.
mod vault;
// Unconditional on every target — see torrent.rs's module doc for why
// (flutter_rust_bridge's generated glue references this module regardless
// of any #[cfg] on its own declaration). librqbit itself is still gated to
// non-wasm32 in Cargo.toml; torrent.rs's internals mirror that.
/// The one collection model the app renders — see its module doc. Replaces
/// the former `collab` module, which was one half of a pair that was never
/// joined.
pub mod collections;
pub mod device;
/// Every knob librqbit exposes, persisted and bridged — see its module doc.
pub mod settings;
pub mod torrent;
