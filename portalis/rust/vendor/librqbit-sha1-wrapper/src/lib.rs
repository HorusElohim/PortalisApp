// Portable SHA-1/SHA-256 implementations for librqbit.
//
// The public traits are intentionally small so librqbit can swap hashing
// implementations without changing its torrent code. The `sha1-ring` feature
// name is retained for librqbit 9 compatibility; this patched implementation
// uses pure Rust instead of AWS-LC or a platform crypto library so Android and
// Apple cross-compilation do not require OpenSSL or native crypto objects.

pub trait ISha1 {
    fn new() -> Self;
    fn update(&mut self, buf: &[u8]);
    fn finish(self) -> [u8; 20];
}

pub trait ISha256 {
    fn new() -> Self;
    fn update(&mut self, buf: &[u8]);
    fn finish(self) -> [u8; 32];

    fn finish_id32(self) -> [u8; 32]
    where
        Self: Sized,
    {
        self.finish()
    }
}

assert_cfg::exactly_one! {
    feature = "sha1-crypto-hash",
    feature = "sha1-ring",
}

#[cfg(feature = "sha1-crypto-hash")]
mod crypto_hash_impl {
    use super::{ISha1, ISha256};

    pub struct Sha1CryptoHash {
        inner: crypto_hash::Hasher,
    }

    impl ISha1 for Sha1CryptoHash {
        fn new() -> Self {
            Self {
                inner: crypto_hash::Hasher::new(crypto_hash::Algorithm::SHA1),
            }
        }

        fn update(&mut self, buf: &[u8]) {
            use std::io::Write;
            self.inner.write_all(buf).unwrap();
        }

        fn finish(mut self) -> [u8; 20] {
            let result = self.inner.finish();
            debug_assert_eq!(result.len(), 20);
            let mut output = [0u8; 20];
            output.copy_from_slice(&result);
            output
        }
    }

    pub struct Sha256CryptoHash {
        inner: crypto_hash::Hasher,
    }

    impl ISha256 for Sha256CryptoHash {
        fn new() -> Self {
            Self {
                inner: crypto_hash::Hasher::new(crypto_hash::Algorithm::SHA256),
            }
        }

        fn update(&mut self, buf: &[u8]) {
            use std::io::Write;
            self.inner.write_all(buf).unwrap();
        }

        fn finish(mut self) -> [u8; 32] {
            let result = self.inner.finish();
            debug_assert_eq!(result.len(), 32);
            let mut output = [0u8; 32];
            output.copy_from_slice(&result);
            output
        }
    }
}

#[cfg(feature = "sha1-ring")]
mod portable_impl {
    use super::{ISha1, ISha256};
    use sha1::Digest as Sha1Digest;

    pub struct Sha1Portable(sha1::Sha1);

    impl ISha1 for Sha1Portable {
        fn new() -> Self {
            Self(sha1::Sha1::new())
        }

        fn update(&mut self, buf: &[u8]) {
            self.0.update(buf);
        }

        fn finish(self) -> [u8; 20] {
            self.0.finalize().into()
        }
    }

    pub struct Sha256Portable(sha2::Sha256);

    impl ISha256 for Sha256Portable {
        fn new() -> Self {
            Self(sha2::Sha256::new())
        }

        fn update(&mut self, buf: &[u8]) {
            self.0.update(buf);
        }

        fn finish(self) -> [u8; 32] {
            self.0.finalize().into()
        }
    }
}

#[cfg(feature = "sha1-crypto-hash")]
pub type Sha1 = crypto_hash_impl::Sha1CryptoHash;
#[cfg(feature = "sha1-ring")]
pub type Sha1 = portable_impl::Sha1Portable;
#[cfg(feature = "sha1-crypto-hash")]
pub type Sha256 = crypto_hash_impl::Sha256CryptoHash;
#[cfg(feature = "sha1-ring")]
pub type Sha256 = portable_impl::Sha256Portable;

#[cfg(test)]
mod tests {
    use super::{ISha1, ISha256, Sha1, Sha256};

    #[test]
    fn known_vectors_are_stable() {
        let mut sha1 = Sha1::new();
        sha1.update(b"");
        assert_eq!(
            sha1.finish(),
            [
                0xda, 0x39, 0xa3, 0xee, 0x5e, 0x6b, 0x4b, 0x0d, 0x32, 0x55, 0xbf, 0xef,
                0x95, 0x60, 0x18, 0x90, 0xaf, 0xd8, 0x07, 0x09,
            ]
        );

        let mut sha256 = Sha256::new();
        sha256.update(b"");
        assert_eq!(
            sha256.finish(),
            [
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99,
                0x6f, 0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95,
                0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
            ]
        );
    }
}
