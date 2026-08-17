//! Backend-owned entry point for the portable Nexus client.
//!
//! Keep this module below the Flutter bridge for now: protobuf envelopes and
//! transport handles are implementation details, while the eventual Dart API
//! should speak in collection/share operations.

use portalis_nexus_client::{DeviceSigner, EndpointAddr, NexusClient, TransportError};
use portalis_nexus_protocol::{MAX_USERNAME_CHARS, MIN_USERNAME_CHARS};
use rand::{rngs::OsRng, RngCore};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::domain::identity::DeviceIdentity;

/// The two independent device keys the Nexus workflow needs.
///
/// The existing Ed25519 identity remains the signing authority used by the
/// legacy collection model. X25519 is generated beside it and only receives
/// sealed share keys; neither private key crosses this backend boundary.
pub(crate) struct NexusIdentity {
    signing: DeviceIdentity,
    encryption: StaticSecret,
}

impl NexusIdentity {
    pub(crate) fn generate(signing: DeviceIdentity) -> Self {
        let mut encryption = [0_u8; 32];
        OsRng.fill_bytes(&mut encryption);
        Self {
            signing,
            encryption: StaticSecret::from(encryption),
        }
    }

    pub(crate) fn from_parts(signing: DeviceIdentity, encryption: [u8; 32]) -> Self {
        Self {
            signing,
            encryption: StaticSecret::from(encryption),
        }
    }

    pub(crate) fn signing_identity(&self) -> &DeviceIdentity {
        &self.signing
    }

    pub(crate) fn encryption_secret(&self) -> [u8; 32] {
        self.encryption.to_bytes()
    }
}

impl DeviceSigner for NexusIdentity {
    fn public_key(&self) -> [u8; 32] {
        self.signing.public_key()
    }

    fn encryption_public_key(&self) -> [u8; 32] {
        PublicKey::from(&self.encryption).to_bytes()
    }

    fn sign(&self, payload: &[u8]) -> [u8; 64] {
        self.signing.sign(payload).to_bytes()
    }
}

/// Opens the supervised client used by the online collection workflow.
#[allow(dead_code)]
pub(crate) async fn connect(endpoint: EndpointAddr) -> Result<NexusClient, TransportError> {
    NexusClient::connect(endpoint).await
}

/// Opens the configured Nexus service when the person has set one up.
///
/// An absent configuration is normal on a first run and is not treated as a
/// failed network connection. The app lifecycle will own the returned client
/// when online collection workflows begin.
#[allow(dead_code)]
pub(crate) async fn connect_configured() -> anyhow::Result<Option<NexusClient>> {
    let Some(endpoint) = crate::nexus_settings::nexus_endpoint_config()?.endpoint_addr()? else {
        return Ok(None);
    };
    Ok(Some(connect(endpoint).await?))
}

/// Folds what the device is called into something the service will accept.
///
/// The service's rule is letters, digits and underscores, 3 to 24 characters.
/// A device name is a sentence a person wrote — "Ada's MacBook Pro" — so the
/// runs between the acceptable characters collapse to single underscores
/// rather than vanishing, which keeps the words apart and the result readable.
///
/// A name with nothing usable in it still has to produce a handle, because
/// refusing to register would leave the device anonymous over a display
/// detail. `device` is the fallback, and the discriminator the service
/// appends is what actually distinguishes it.
#[must_use]
pub(crate) fn username_from(device_name: &str) -> String {
    let mut username = String::new();
    for character in device_name.chars() {
        if character.is_alphanumeric() || character == '_' {
            username.push(character);
        } else if !username.ends_with('_') && !username.is_empty() {
            username.push('_');
        }
        if username.chars().count() == MAX_USERNAME_CHARS {
            break;
        }
    }
    let username = username.trim_matches('_');
    if username.chars().count() < MIN_USERNAME_CHARS {
        return "device".to_owned();
    }
    username.to_owned()
}

/// Says who this device is, and answers with the handle the service knows it
/// by — enrolling it first if this is the first time.
///
/// One request, whether or not this device has registered before. A
/// connection is issued one challenge and may spend it once, so asking
/// "authenticate, and register if that fails" would spend the only attempt
/// discovering which applied; and keeping a local "already registered" flag
/// to decide would be a second copy of a fact the service owns, free to drift
/// the moment a store is restored from a backup or pointed somewhere else.
/// Registration is idempotent for exactly this reason.
///
/// The username derived here therefore only matters on the first call. After
/// that the service answers with the handle it already assigned, because a
/// handle cannot be changed.
pub(crate) async fn identify<S: DeviceSigner + ?Sized>(
    client: &NexusClient,
    signer: &S,
    device_name: &str,
) -> Result<String, TransportError> {
    let identity = client.register(&username_from(device_name), signer).await?;
    Ok(format!("{}#{}", identity.username, identity.discriminator))
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    use super::*;

    #[test]
    fn existing_signing_identity_drives_the_portable_client() {
        let identity = NexusIdentity::generate(DeviceIdentity::from_bytes(&[7; 32]));
        let payload = b"nexus registration payload";

        assert_eq!(
            identity.public_key(),
            identity.signing_identity().public_key()
        );
        assert_ne!(DeviceSigner::device_id(&identity), identity.public_key());
        assert!(VerifyingKey::from_bytes(&identity.public_key())
            .unwrap()
            .verify(payload, &Signature::from_bytes(&identity.sign(payload)))
            .is_ok());
    }

    /// What a person calls their device is not what the service will accept,
    /// and the gap has to close without anybody being asked about it.
    #[test]
    fn a_device_name_becomes_a_username_the_service_accepts() {
        assert_eq!(username_from("Ada's MacBook Pro"), "Ada_s_MacBook_Pro");
        assert_eq!(
            username_from("  spaces  everywhere  "),
            "spaces_everywhere",
            "leading and trailing runs leave no underscore hanging off either end"
        );
        assert_eq!(
            username_from("réservé"),
            "réservé",
            "letters outside ASCII are letters"
        );
        assert_eq!(
            username_from("!!!"),
            "device",
            "a name with nothing usable in it still has to register"
        );
        assert_eq!(
            username_from("no"),
            "device",
            "too short to be a username, and the service would refuse it"
        );

        let long = username_from("Ada Lovelace's Extremely Well Named Laptop");
        assert!(
            long.chars().count() <= MAX_USERNAME_CHARS,
            "{long} is longer than the service allows"
        );

        for name in ["Ada's MacBook Pro", "!!!", "réservé", "  spaces  ", "no"] {
            let username = username_from(name);
            let length = username.chars().count();
            assert!(
                (MIN_USERNAME_CHARS..=MAX_USERNAME_CHARS).contains(&length),
                "{name} became {username}, which is {length} characters"
            );
            assert!(
                username
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '_'),
                "{name} became {username}, which the charset rule refuses"
            );
        }
    }

    #[test]
    fn encryption_key_round_trips_independently() {
        let identity = NexusIdentity::from_parts(DeviceIdentity::from_bytes(&[9; 32]), [11; 32]);
        let restored = NexusIdentity::from_parts(
            DeviceIdentity::from_bytes(&identity.signing_identity().to_bytes()),
            identity.encryption_secret(),
        );

        assert_eq!(restored.public_key(), identity.public_key());
        assert_eq!(
            restored.encryption_public_key(),
            identity.encryption_public_key()
        );
    }
}
