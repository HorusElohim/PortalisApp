//! Device identity, exposed to Flutter for the first time (previously only
//! `domain::identity` existed, unused by anything). Backs the User screen:
//! a real Ed25519 keypair generated once and persisted locally, with an
//! editable nickname — no accounts, no servers, matching the design in
//! `rust/backend/README.md`.
//!
//! Unconditional signatures for the same reason as `torrent.rs`: FRB's
//! generated glue references `crate::device::*` regardless of any `#[cfg]`
//! on this module's own declaration.

#[derive(Debug, Clone)]
pub struct DeviceIdentityInfo {
    pub device_id: String,
    pub nickname: String,
}

/// Loads the persisted identity, generating and saving one on first call.
pub fn device_identity() -> anyhow::Result<DeviceIdentityInfo> {
    native::device_identity()
}

/// Renames this device's identity (display name only — the keypair itself
/// never changes).
pub fn set_nickname(nickname: String) -> anyhow::Result<DeviceIdentityInfo> {
    native::set_nickname(nickname)
}

/// This device's signing identity, kept below the bridge so callers can sign
/// and open envelopes without exposing key material.
pub(crate) fn current_signing_identity() -> anyhow::Result<crate::domain::identity::DeviceIdentity>
{
    native::load_or_create().map(|(identity, _nickname)| identity)
}

mod native {
    use std::sync::Mutex;

    use anyhow::Context;
    use serde::{Deserialize, Serialize};

    use crate::domain::identity::DeviceIdentity;

    use super::DeviceIdentityInfo;

    #[derive(Serialize, Deserialize)]
    struct PersistedIdentity {
        /// Hex-encoded 32-byte Ed25519 signing key.
        secret_key_hex: String,
        nickname: String,
    }

    static CACHE: Mutex<Option<DeviceIdentityInfo>> = Mutex::new(None);

    #[cfg(test)]
    pub(super) fn forget_cache_for_test() {
        *CACHE.lock().unwrap() = None;
    }

    fn vault() -> crate::vault::Vault {
        crate::vault::Vault::named("identity.json")
    }

    pub(super) fn load_or_create() -> anyhow::Result<(DeviceIdentity, String)> {
        if let Some(persisted) = vault().read::<PersistedIdentity>()? {
            return restore(persisted);
        }

        let identity = DeviceIdentity::generate();
        let nickname = "Me".to_string();
        crate::log::clog!(
            "device",
            "no identity yet, generated one, device_id={}…",
            &identity.device_id().to_hex()[..8]
        );
        save(&identity, &nickname)?;
        Ok((identity, nickname))
    }

    fn restore(persisted: PersistedIdentity) -> anyhow::Result<(DeviceIdentity, String)> {
        let key: [u8; 32] = hex::decode(&persisted.secret_key_hex)
            .context("decoding stored secret key")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("stored secret key is not 32 bytes"))?;
        let identity = DeviceIdentity::from_bytes(&key);
        Ok((identity, persisted.nickname))
    }

    fn save(identity: &DeviceIdentity, nickname: &str) -> anyhow::Result<()> {
        vault().write(&PersistedIdentity {
            secret_key_hex: hex::encode(identity.to_bytes()),
            nickname: nickname.to_string(),
        })
    }

    pub(super) fn device_identity() -> anyhow::Result<DeviceIdentityInfo> {
        let mut cache = CACHE.lock().unwrap();
        if let Some(info) = cache.as_ref() {
            return Ok(info.clone());
        }
        let (identity, nickname) = load_or_create()?;
        let info = DeviceIdentityInfo {
            device_id: identity.device_id().to_hex(),
            nickname,
        };
        *cache = Some(info.clone());
        Ok(info)
    }

    pub(super) fn set_nickname(nickname: String) -> anyhow::Result<DeviceIdentityInfo> {
        let (identity, _old_nickname) = load_or_create()?;
        save(&identity, &nickname)?;
        let info = DeviceIdentityInfo {
            device_id: identity.device_id().to_hex(),
            nickname,
        };
        *CACHE.lock().unwrap() = Some(info.clone());
        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::*;

    #[derive(Serialize)]
    struct LegacyPersistedIdentity {
        secret_key_hex: String,
        nickname: String,
    }

    /// Losing this file loses the identity every signature was made under —
    /// there is no recovery, so the round trip is worth asserting on disk.
    #[test]
    fn the_identity_survives_a_reload() {
        let _temp = crate::paths::redirect_to_temp();
        let first = native::load_or_create().unwrap().0.device_id();

        native::forget_cache_for_test();

        assert_eq!(native::load_or_create().unwrap().0.device_id(), first);
    }

    /// A signing key written before the (now-removed) transport encryption
    /// key existed still loads: only the Ed25519 half was ever load-bearing
    /// for anything this backend still does.
    #[test]
    fn an_existing_signing_identity_survives_across_a_format_change() {
        let _temp = crate::paths::redirect_to_temp();
        let signing_secret = [13_u8; 32];
        crate::vault::Vault::named("identity.json")
            .write(&LegacyPersistedIdentity {
                secret_key_hex: hex::encode(signing_secret),
                nickname: "Maya".into(),
            })
            .unwrap();

        let (loaded, nickname) = native::load_or_create().unwrap();
        assert_eq!(loaded.to_bytes(), signing_secret);
        assert_eq!(nickname, "Maya");
    }
}
