//! Process configuration read at startup.

use std::fmt;
use std::net::{AddrParseError, SocketAddr};

pub const DEFAULT_LISTEN_ADDR: &str = "127.0.0.1:8080";
/// The authority used when none is configured, matching local development.
pub const DEFAULT_SERVER_AUTHORITY: &str = "127.0.0.1:8080";

/// The database used when none is configured.
pub const DEFAULT_DATABASE: &str = "portalis_nexus";

/// Which engine to run, chosen rather than assumed.
///
/// One binary, two engines (D5). A self-hoster sets a directory and gets a few
/// files; an operator already running `MongoDB` sets a URI. Neither is the
/// default, because a service that silently picks its own storage is a service
/// somebody has to read the source of to understand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Storage {
    /// A directory of files. No server, no replica set.
    Embedded { data_dir: std::path::PathBuf },
    /// A `MongoDB` deployment, which needs a replica set for transactions.
    Mongo { uri: String, database: String },
}

/// The production server cannot run without durable identity storage, and
/// will not guess which kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingStorage;

impl fmt::Display for MissingStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "set PORTALIS_NEXUS_DATA_DIR for the embedded engine, \
             or PORTALIS_NEXUS_MONGODB_URI for MongoDB",
        )
    }
}

impl std::error::Error for MissingStorage {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    pub listen_addr: SocketAddr,
    /// The host clients believe they are talking to. Every signature is bound
    /// to it, so a deployment reached by any other name refuses them all;
    /// behind a proxy or in a container this is not the listen address, which
    /// is why it is configured rather than derived.
    pub server_authority: String,
    /// Where durable state lives. The server process refuses to start without
    /// this value; retaining the optional representation keeps configuration
    /// parsing independently testable.
    pub mongodb_uri: Option<String>,
    /// Where the embedded engine keeps its files.
    pub data_dir: Option<std::path::PathBuf>,
    pub database: String,
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
            // The address dialled and the address bound are the same thing
            // for local development, and nowhere else.
            server_authority: listen_addr.to_string(),
            mongodb_uri: None,
            data_dir: None,
            database: DEFAULT_DATABASE.to_owned(),
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
            server_authority: lookup("PORTALIS_NEXUS_SERVER_AUTHORITY")
                .unwrap_or(defaults.server_authority),
            mongodb_uri: lookup("PORTALIS_NEXUS_MONGODB_URI"),
            data_dir: lookup("PORTALIS_NEXUS_DATA_DIR").map(std::path::PathBuf::from),
            database: lookup("PORTALIS_NEXUS_DATABASE")
                .unwrap_or_else(|| DEFAULT_DATABASE.to_owned()),
            listen_addr: defaults.listen_addr,
        })
    }

    /// Returns the durable store URI required by the server process.
    ///
    /// # Errors
    ///
    /// Returns [`MissingStorage`] when neither engine was configured.
    ///
    /// The embedded engine wins if both are set, because it is the one that
    /// needs nothing else running — a machine with both configured is one
    /// being moved, and the local files are the safer half to believe.
    pub fn storage(&self) -> Result<Storage, MissingStorage> {
        if let Some(data_dir) = &self.data_dir {
            return Ok(Storage::Embedded {
                data_dir: data_dir.clone(),
            });
        }
        self.mongodb_uri
            .as_deref()
            .map(|uri| Storage::Mongo {
                uri: uri.to_owned(),
                database: self.database.clone(),
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

        assert_eq!(config.mongodb_uri, None);
        assert_eq!(config.database, DEFAULT_DATABASE);
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
        assert_eq!(config.server_authority, DEFAULT_LISTEN_ADDR);
        assert_eq!(config.mongodb_uri, None);
        assert_eq!(config.database, DEFAULT_DATABASE);
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
            ("PORTALIS_NEXUS_SERVER_AUTHORITY", "nexus.example"),
            ("PORTALIS_NEXUS_MONGODB_URI", "mongodb://example/"),
            ("PORTALIS_NEXUS_DATABASE", "custom_db"),
        ];
        let config = ServerConfig::from_lookup(|name| {
            set.iter()
                .find(|(variable, _)| *variable == name)
                .map(|(_, value)| (*value).to_owned())
        })
        .expect("every value is valid");

        assert_eq!(config.listen_addr.to_string(), "0.0.0.0:9000");
        assert_eq!(config.server_authority, "nexus.example");
        assert_eq!(config.mongodb_uri.as_deref(), Some("mongodb://example/"));
        assert_eq!(config.database, "custom_db");
        assert_eq!(
            config.storage(),
            Ok(Storage::Mongo {
                uri: "mongodb://example/".to_owned(),
                database: "custom_db".to_owned()
            })
        );
    }

    /// Both engines configured means a machine being moved, and the local
    /// files are the safer half to believe.
    #[test]
    fn the_embedded_engine_wins_when_both_are_configured() {
        let config = ServerConfig {
            mongodb_uri: Some("mongodb://example/".to_owned()),
            data_dir: Some(std::path::PathBuf::from("/var/lib/portalis")),
            ..ServerConfig::from_listen_value(None).expect("the default address is valid")
        };

        assert_eq!(
            config.storage(),
            Ok(Storage::Embedded {
                data_dir: std::path::PathBuf::from("/var/lib/portalis")
            })
        );
    }

    /// Bound and dialled are the same only for local development. A container
    /// binds `0.0.0.0` and is never reached by that name, so leaving the
    /// authority to follow the listen address would refuse every signature.
    #[test]
    fn the_authority_follows_the_listen_address_until_it_is_set() {
        let derived = ServerConfig::from_listen_value(Some("0.0.0.0:9000")).expect("valid");
        assert_eq!(derived.server_authority, "0.0.0.0:9000");

        let configured = ServerConfig::from_lookup(|name| match name {
            "PORTALIS_NEXUS_LISTEN_ADDR" => Some("0.0.0.0:9000".to_owned()),
            "PORTALIS_NEXUS_SERVER_AUTHORITY" => Some("nexus.example:443".to_owned()),
            _ => None,
        })
        .expect("valid");

        assert_eq!(configured.listen_addr.to_string(), "0.0.0.0:9000");
        assert_eq!(configured.server_authority, "nexus.example:443");
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
