//! Process configuration read at startup.

use std::fmt;
use std::net::{AddrParseError, SocketAddr};

pub const DEFAULT_LISTEN_ADDR: &str = "127.0.0.1:8080";

/// Which engine to run, chosen rather than assumed.
///
/// One binary, one engine (ADR-0002): an operator sets a directory and gets a
/// few files. It is not defaulted, because a service that silently picks its
/// own storage location is a service somebody has to read the source of to
/// understand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Storage {
    /// A directory of files. No server, no replica set.
    Embedded { data_dir: std::path::PathBuf },
}

/// The production server cannot run without durable identity storage, and
/// will not guess which kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingStorage;

impl fmt::Display for MissingStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("set PORTALIS_NEXUS_DATA_DIR for the embedded engine")
    }
}

impl std::error::Error for MissingStorage {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    pub listen_addr: SocketAddr,
    /// Where the embedded engine keeps its files.
    ///
    /// The server process refuses to start without this value; retaining the
    /// optional representation keeps configuration parsing independently
    /// testable.
    pub data_dir: Option<std::path::PathBuf>,
    /// The hexadecimal 32-byte Iroh private key supplied by an operator.
    ///
    /// When omitted, a deployment generates and keeps the same secret beside
    /// its data.
    pub node_secret: Option<String>,
}

impl ServerConfig {
    /// Parses the configured address or applies the local development default.
    ///
    /// # Errors
    ///
    /// Returns [`AddrParseError`] when the supplied address is malformed.
    pub fn from_listen_value(value: Option<&str>) -> Result<Self, AddrParseError> {
        let listen_addr = value.unwrap_or(DEFAULT_LISTEN_ADDR).parse()?;
        Ok(Self {
            listen_addr,
            data_dir: None,
            node_secret: None,
        })
    }

    /// Reads the whole configuration from the environment.
    ///
    /// # Errors
    ///
    /// Returns [`AddrParseError`] when the listen address is malformed.
    pub fn from_environment() -> Result<Self, AddrParseError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Builds a configuration from an arbitrary variable lookup, so the
    /// precedence and defaulting rules can be tested without touching the
    /// real process environment, which every test would otherwise share.
    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, AddrParseError> {
        let listen = lookup("PORTALIS_NEXUS_LISTEN_ADDR");
        let defaults = Self::from_listen_value(listen.as_deref())?;
        Ok(Self {
            data_dir: lookup("PORTALIS_NEXUS_DATA_DIR").map(std::path::PathBuf::from),
            node_secret: lookup("PORTALIS_NEXUS_NODE_SECRET"),
            listen_addr: defaults.listen_addr,
        })
    }

    /// Returns the durable store location required by the server process.
    ///
    /// # Errors
    ///
    /// Returns [`MissingStorage`] when no data directory was configured.
    pub fn storage(&self) -> Result<Storage, MissingStorage> {
        self.data_dir
            .as_ref()
            .map(|data_dir| Storage::Embedded {
                data_dir: data_dir.clone(),
            })
            .ok_or(MissingStorage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_default_address() {
        let config = ServerConfig::from_listen_value(None).expect("valid default address");

        assert_eq!(config.listen_addr.to_string(), DEFAULT_LISTEN_ADDR);
    }

    #[test]
    fn accepts_custom_address() {
        let config =
            ServerConfig::from_listen_value(Some("0.0.0.0:9000")).expect("valid custom address");

        assert_eq!(config.listen_addr.to_string(), "0.0.0.0:9000");
    }

    #[test]
    fn rejects_invalid_address() {
        assert!(ServerConfig::from_listen_value(Some("not-an-address")).is_err());
    }

    #[test]
    fn leaves_the_database_uri_unset_when_not_configured() {
        let config = ServerConfig::from_listen_value(None).expect("valid default address");

        assert_eq!(config.data_dir, None);
        assert_eq!(config.node_secret, None);
        assert_eq!(config.storage(), Err(MissingStorage));
        assert!(
            MissingStorage
                .to_string()
                .contains("PORTALIS_NEXUS_DATA_DIR"),
            "and says how to fix it"
        );
    }

    #[test]
    fn defaults_every_variable_that_is_unset() {
        let config = ServerConfig::from_lookup(|_name| None).expect("defaults are valid");

        assert_eq!(config.listen_addr.to_string(), DEFAULT_LISTEN_ADDR);
        assert_eq!(config.data_dir, None);
        assert_eq!(config.node_secret, None);
        assert_eq!(config.storage(), Err(MissingStorage));
        assert!(
            MissingStorage
                .to_string()
                .contains("PORTALIS_NEXUS_DATA_DIR"),
            "and says how to fix it"
        );
    }

    #[test]
    fn reads_every_variable_that_is_set() {
        let set = [
            ("PORTALIS_NEXUS_LISTEN_ADDR", "0.0.0.0:9000"),
            ("PORTALIS_NEXUS_DATA_DIR", "/var/lib/portalis"),
            (
                "PORTALIS_NEXUS_NODE_SECRET",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
        ];
        let config = ServerConfig::from_lookup(|name| {
            set.iter()
                .find(|(variable, _)| *variable == name)
                .map(|(_, value)| (*value).to_owned())
        })
        .expect("every value is valid");

        assert_eq!(config.listen_addr.to_string(), "0.0.0.0:9000");
        assert_eq!(
            config.data_dir.as_deref(),
            Some(std::path::Path::new("/var/lib/portalis"))
        );
        assert_eq!(
            config.node_secret.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_eq!(
            config.storage(),
            Ok(Storage::Embedded {
                data_dir: std::path::PathBuf::from("/var/lib/portalis")
            })
        );
    }

    /// The only path that touches the real process environment. Asserting it
    /// agrees with an equivalent explicit lookup keeps the test deterministic
    /// whatever the ambient environment happens to hold.
    #[test]
    fn reads_the_process_environment() {
        let expected = ServerConfig::from_lookup(|name| std::env::var(name).ok());

        assert_eq!(ServerConfig::from_environment(), expected);
    }

    #[test]
    fn rejects_an_invalid_address_from_the_environment() {
        let result = ServerConfig::from_lookup(|name| {
            (name == "PORTALIS_NEXUS_LISTEN_ADDR").then(|| "not-an-address".to_owned())
        });

        assert!(result.is_err());
    }
}
