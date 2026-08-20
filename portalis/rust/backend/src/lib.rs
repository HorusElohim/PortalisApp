#![cfg_attr(not(frb_expand), allow(unexpected_cfgs))]
mod api; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */
/// The Flutter-facing version handshake.
pub mod bridge;
/// The one collection model the app renders — see its module doc. Replaces
/// the former `collab` module, which was one half of a pair that was never
/// joined.
pub mod nexus;
/// The single app-facing Nexus lifecycle, streams, and command boundary.
pub mod portalis_api;
