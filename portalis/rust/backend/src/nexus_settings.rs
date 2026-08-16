//! The trusted Nexus service address the app can connect to.
//!
//! This is deliberately separate from [`crate::settings`]: torrent settings
//! tune a local engine, while this identifies a remote service. A host and
//! port alone are not a Nexus identity. The public QUIC Node ID is what Iroh
//! authenticates; the direct address is only a route to that identity.

use std::net::SocketAddr;
use std::str::FromStr;

use anyhow::{bail, Context as _};
use portalis_nexus_client::{EndpointAddr, EndpointId};
use serde::{Deserialize, Serialize};

/// A trusted Nexus service the app may connect to.
///
/// The Node ID is the identity and is required; the direct address is an
/// optional hint. Discovery resolves a Node ID to an address on its own — over
/// mDNS on the same network, or a signed record on n0's name server anywhere
/// else — so an address is worth setting only to skip that lookup, or to reach
/// a service that publishes neither.
///
/// An address without a Node ID stays refused. A route to an unnamed service
/// discards the identity QUIC authenticates, which is the only thing making
/// the service the one the person meant.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NexusEndpointConfig {
    /// The public QUIC Node ID logged by the Nexus service.
    pub node_id: Option<String>,
    /// An IP address and UDP port where that service can currently be reached,
    /// when discovery should not be relied on to find it.
    pub direct_address: Option<String>,
}

impl NexusEndpointConfig {
    /// Returns the typed Iroh address, or `None` before Nexus is configured.
    ///
    /// # Errors
    ///
    /// Returns an error when an address is set without a Node ID, the Node ID
    /// is malformed, or the direct address is not an IP socket address.
    pub(crate) fn endpoint_addr(&self) -> anyhow::Result<Option<EndpointAddr>> {
        let Some(node_id) = present(&self.node_id) else {
            if present(&self.direct_address).is_some() {
                bail!("set the Nexus Node ID as well as the direct address, or clear both");
            }
            return Ok(None);
        };
        let node_id = EndpointId::from_str(node_id).context("the Nexus Node ID is not valid")?;
        let Some(direct_address) = present(&self.direct_address) else {
            // No address: discovery is expected to find it by Node ID.
            return Ok(Some(EndpointAddr::new(node_id)));
        };
        let direct_address = direct_address
            .parse::<SocketAddr>()
            .context("the Nexus direct address must be an IP address and UDP port")?;
        Ok(Some(
            EndpointAddr::new(node_id).with_direct_addresses([direct_address]),
        ))
    }

    fn normalized(mut self) -> anyhow::Result<Self> {
        self.node_id = normalise(self.node_id);
        self.direct_address = normalise(self.direct_address);
        self.endpoint_addr()?;
        Ok(self)
    }
}

/// Loads the saved Nexus endpoint, or an unconfigured value on first run.
///
/// # Errors
///
/// Returns an error when the saved configuration cannot be read or is invalid.
pub fn nexus_endpoint_config() -> anyhow::Result<NexusEndpointConfig> {
    let config: NexusEndpointConfig = vault().read()?.unwrap_or_default();
    config.normalized()
}

/// Validates and persists the Nexus endpoint used by future app connections.
///
/// # Errors
///
/// Returns an error when the identity and route are incomplete or malformed,
/// or the configuration cannot be written.
pub fn set_nexus_endpoint_config(config: NexusEndpointConfig) -> anyhow::Result<()> {
    vault().write(&config.normalized()?)
}

fn vault() -> crate::vault::Vault {
    crate::vault::Vault::named("nexus.json")
}

fn present(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|value| !value.trim().is_empty())
}

fn normalise(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::DeviceIdentity;

    fn node_id() -> String {
        let public_key = DeviceIdentity::from_bytes(&[7; 32]).public_key();
        EndpointId::from_bytes(&public_key)
            .expect("a generated device public key is valid")
            .to_string()
    }

    #[test]
    fn an_absent_configuration_never_invents_a_service() {
        assert_eq!(
            NexusEndpointConfig::default().endpoint_addr().unwrap(),
            None
        );
    }

    #[test]
    fn a_complete_configuration_becomes_an_authenticated_endpoint() {
        let config = NexusEndpointConfig {
            node_id: Some(format!("  {}  ", node_id())),
            direct_address: Some(" 127.0.0.1:7443 ".to_owned()),
        }
        .normalized()
        .unwrap();

        let endpoint = config.endpoint_addr().unwrap().expect("configured");
        assert_eq!(endpoint.node_id.to_string(), node_id());
        assert_eq!(
            endpoint.direct_addresses().next().unwrap().to_string(),
            "127.0.0.1:7443"
        );
    }

    /// A Node ID on its own is a complete configuration: discovery resolves
    /// it to an address, which is the point of publishing one.
    #[test]
    fn a_node_id_alone_is_enough_to_be_configured() {
        let config = NexusEndpointConfig {
            node_id: Some(node_id()),
            direct_address: None,
        };

        let endpoint = config
            .endpoint_addr()
            .expect("a Node ID is a configuration")
            .expect("configured");

        assert_eq!(endpoint.node_id.to_string(), node_id());
        assert_eq!(
            endpoint.direct_addresses().count(),
            0,
            "no address was given, so none should be invented"
        );
    }

    #[test]
    fn an_incomplete_or_malformed_configuration_is_refused() {
        for config in [
            NexusEndpointConfig {
                node_id: None,
                direct_address: Some("127.0.0.1:7443".to_owned()),
            },
            NexusEndpointConfig {
                node_id: Some("not-a-node".to_owned()),
                direct_address: Some("127.0.0.1:7443".to_owned()),
            },
            NexusEndpointConfig {
                node_id: Some(node_id()),
                direct_address: Some("nexus.example:7443".to_owned()),
            },
        ] {
            assert!(config.endpoint_addr().is_err(), "{config:?}");
        }
    }

    #[test]
    fn a_configuration_survives_a_reload() {
        let _temporary_state = crate::paths::redirect_to_temp();
        let expected = NexusEndpointConfig {
            node_id: Some(node_id()),
            direct_address: Some("127.0.0.1:7443".to_owned()),
        };

        set_nexus_endpoint_config(expected.clone()).unwrap();

        assert_eq!(nexus_endpoint_config().unwrap(), expected);
    }
}
