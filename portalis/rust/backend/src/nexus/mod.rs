//! The complete Portalis nexus: all internal machinery lives here.
//!
//! The only public boundary is the `portalis_api` module at the crate root
//! (and the generated `api` + `bridge` modules for FRB). Everything else is
//! an implementation detail reachable through `crate::nexus::...`.

pub mod collections;
pub mod content_location;
pub mod core;
pub mod crypto;
pub mod device;
pub mod domain;
pub mod linked_source_store;
pub mod log;
pub mod paths;
/// Native Android and iOS adapters, isolated from the platform-neutral core.
pub(crate) mod platform;
pub mod projection;
pub mod settings;
pub mod store;
pub mod substrate;
pub mod torrent;
pub mod vault;
