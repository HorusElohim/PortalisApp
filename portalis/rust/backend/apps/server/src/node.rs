//! The service's stable QUIC identity.
//!
//! A node ID is public and derived from this private key. It is not a server
//! name: clients authenticate the node ID in the QUIC handshake, and
//! signatures are scoped to that same identity.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use iroh::SecretKey;
use thiserror::Error;

use crate::ServerConfig;

const NODE_SECRET_FILE: &str = "node-secret";
const NODE_SECRET_BYTES: usize = 32;

#[derive(Debug, Error)]
pub enum NodeSecretError {
    #[error(
        "PORTALIS_NEXUS_NODE_SECRET must be exactly 64 lowercase or uppercase hexadecimal characters"
    )]
    InvalidEnvironment,
    #[error("set PORTALIS_NEXUS_DATA_DIR, or supply PORTALIS_NEXUS_NODE_SECRET")]
    MissingDataDir,
    #[error("could not read the Nexus node secret at {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the Nexus node secret at {path} must contain exactly 32 bytes")]
    InvalidFile { path: PathBuf },
    #[error("could not create the Nexus node secret at {path}")]
    Create {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Loads the operator-managed secret, or creates the embedded service's
/// secret once. The generated file is raw bytes rather than text so it cannot
/// accidentally be copied into a configuration file or log.
///
/// # Errors
///
/// Returns [`NodeSecretError`] when the configured value is malformed, a
/// durable secret cannot be read or created, or no private-key location was
/// configured.
pub fn load_node_secret(config: &ServerConfig) -> Result<SecretKey, NodeSecretError> {
    if let Some(secret) = &config.node_secret {
        return parse_hex_secret(secret).map(|bytes| SecretKey::from_bytes(&bytes));
    }
    let Some(data_dir) = &config.data_dir else {
        return Err(NodeSecretError::MissingDataDir);
    };
    load_or_create(&data_dir.join(NODE_SECRET_FILE)).map(|bytes| SecretKey::from_bytes(&bytes))
}

fn parse_hex_secret(encoded: &str) -> Result<[u8; NODE_SECRET_BYTES], NodeSecretError> {
    if encoded.len() != NODE_SECRET_BYTES * 2 {
        return Err(NodeSecretError::InvalidEnvironment);
    }
    let mut secret = [0_u8; NODE_SECRET_BYTES];
    for (byte, pair) in secret.iter_mut().zip(encoded.as_bytes().chunks_exact(2)) {
        let high = hex_digit(pair[0]).ok_or(NodeSecretError::InvalidEnvironment)?;
        let low = hex_digit(pair[1]).ok_or(NodeSecretError::InvalidEnvironment)?;
        *byte = (high << 4) | low;
    }
    Ok(secret)
}

const fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn load_or_create(path: &Path) -> Result<[u8; NODE_SECRET_BYTES], NodeSecretError> {
    match fs::read(path) {
        Ok(bytes) => return bytes_from_file(path, bytes),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(NodeSecretError::Read {
                path: path.to_owned(),
                source: error,
            });
        }
        Err(_) => {}
    }

    fs::create_dir_all(path.parent().expect("a file has a parent")).map_err(|source| {
        NodeSecretError::Create {
            path: path.to_owned(),
            source,
        }
    })?;
    let mut bytes = [0_u8; NODE_SECRET_BYTES];
    getrandom::fill(&mut bytes).map_err(|source| NodeSecretError::Create {
        path: path.to_owned(),
        source: std::io::Error::other(source),
    })?;

    match create_secret(path, &bytes) {
        Ok(()) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => fs::read(path)
            .map_err(|source| NodeSecretError::Read {
                path: path.to_owned(),
                source,
            })
            .and_then(|existing| bytes_from_file(path, existing)),
        Err(source) => Err(NodeSecretError::Create {
            path: path.to_owned(),
            source,
        }),
    }
}

fn bytes_from_file(
    path: &Path,
    bytes: Vec<u8>,
) -> Result<[u8; NODE_SECRET_BYTES], NodeSecretError> {
    bytes.try_into().map_err(|_| NodeSecretError::InvalidFile {
        path: path.to_owned(),
    })
}

fn create_secret(path: &Path, bytes: &[u8; NODE_SECRET_BYTES]) -> std::io::Result<()> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ServerConfig {
        ServerConfig::from_listen_value(None).expect("default config")
    }

    #[test]
    fn accepts_a_32_byte_hexadecimal_secret() {
        let mut config = config();
        config.node_secret = Some("ab".repeat(32));

        assert_eq!(
            load_node_secret(&config).expect("secret").to_bytes(),
            [0xab; 32]
        );
    }

    #[test]
    fn an_operator_managed_secret_takes_precedence_over_embedded_storage() {
        let mut config = config();
        config.data_dir = Some(std::env::temp_dir().join("missing-portalis-node-secret"));
        config.node_secret = Some("cd".repeat(32));

        assert_eq!(
            load_node_secret(&config)
                .expect("environment secret")
                .to_bytes(),
            [0xcd; 32]
        );
    }

    #[test]
    fn refuses_a_malformed_environment_secret() {
        let mut config = config();
        config.node_secret = Some("not a secret".to_owned());

        assert!(matches!(
            load_node_secret(&config),
            Err(NodeSecretError::InvalidEnvironment)
        ));
    }

    #[test]
    fn creates_and_reuses_an_embedded_secret() {
        let directory = std::env::temp_dir().join(format!(
            "portalis-node-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock is after the Unix epoch")
                .as_nanos()
        ));
        let mut config = config();
        config.data_dir = Some(directory.clone());

        let first = load_node_secret(&config).expect("creates a secret");
        let second = load_node_secret(&config).expect("reuses it");

        assert_eq!(first.to_bytes(), second.to_bytes());
        assert_eq!(
            fs::metadata(directory.join(NODE_SECRET_FILE))
                .expect("file")
                .len(),
            32
        );
        fs::remove_dir_all(directory).expect("removes test directory");
    }
}
