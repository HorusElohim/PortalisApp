//! Small top-level FRB-facing functions that don't belong to any specific
//! bridged module. Exists so `flutter_rust_bridge_codegen`'s `--rust-input`
//! can list explicit module paths (`crate::bridge,crate::torrent`, see
//! `tool/frb_build.sh`) instead of the bare `crate` wildcard — which walks
//! every `mod` declaration in the crate regardless of visibility and would
//! sweep up internal-only modules like `domain` too and fail to compile
//! (private fields it assumes are bridgeable). See rust/backend/README.md's
//! "Flutter boundary API".

use crate::nexus::device::{
    DeviceIdentityInfo, device_identity as device_identity_fn, set_nickname as set_nickname_fn,
};
use flutter_rust_bridge::frb;

// Keep web simple by making this a synchronous, non-threaded function.
// FRB will generate a sync binding that avoids web worker/threadpool usage.
#[frb(sync)]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Loads the persisted identity, generating and saving one on first call.
#[frb(sync)]
pub fn device_identity() -> Result<DeviceIdentityInfo, String> {
    device_identity_fn().map_err(|e| e.to_string())
}

/// Renames this device's identity (display name only — the keypair itself
/// never changes).
#[frb(sync)]
pub fn set_nickname(nickname: String) -> Result<DeviceIdentityInfo, String> {
    set_nickname_fn(nickname).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_version_matches_crate_metadata() {
        assert_eq!(get_version(), env!("CARGO_PKG_VERSION"));
    }
}
