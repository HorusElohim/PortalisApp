use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;

/// A device's public identity — stable across IP/network changes, used as
/// the "who" in collaborator lists and manifest entry authorship.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(VerifyingKey);

impl DeviceId {
    pub fn as_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.as_bytes())
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({}…)", &self.to_hex()[..8])
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// A device's full keypair. The private half never leaves this type —
/// everything else in the domain only ever sees a [`DeviceId`] or a
/// `Signature`, never the signing key itself. Persistence is a `KeyStore`
/// adapter's job, not this type's — `to_bytes`/`from_bytes` are the seam.
pub struct DeviceIdentity {
    signing_key: SigningKey,
}

impl DeviceIdentity {
    pub fn generate() -> Self {
        Self {
            signing_key: SigningKey::generate(&mut OsRng),
        }
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(bytes),
        }
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    pub fn device_id(&self) -> DeviceId {
        DeviceId(self.signing_key.verifying_key())
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_bytes_round_trip_to_same_device_id() {
        let identity = DeviceIdentity::generate();
        let bytes = identity.to_bytes();
        let restored = DeviceIdentity::from_bytes(&bytes);

        assert_eq!(identity.device_id(), restored.device_id());
    }
}
