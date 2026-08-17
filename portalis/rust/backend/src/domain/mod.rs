//! What Nexus's own identity is made of.
//!
//! Everything here is `pub(crate)`, not `pub` — these types never cross the
//! FFI boundary directly (see the README's "Flutter boundary API" section).
//! `flutter_rust_bridge`'s codegen scans for the bare `pub` keyword on
//! individual items rather than resolving true Rust visibility, so it would
//! otherwise try to bridge them regardless of `domain` being private.
//!
//! The collaboration types that used to live beside `identity` — invites,
//! manifests, the legacy collection and its collaborators — went with the
//! legacy stack they served.

pub(crate) mod identity;
