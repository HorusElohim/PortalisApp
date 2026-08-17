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

/// The service this build talks to when nobody has chosen one.
///
/// A Node ID is a public key, so shipping it is pinning rather than a secret
/// left lying about: the app then trusts exactly one service identity and
/// cannot be talked out of it. Asking a person to paste one is the weaker
/// arrangement — somebody who can be told to paste a Node ID can be told to
/// paste an impostor's.
///
/// The identity is pinned and the address is not, because the address is the
/// part that changes. Discovery resolves the Node ID to wherever the service
/// is now, so a build does not go stale when a service moves.
const DEFAULT_NODE_ID: Option<&str> = option_env!("PORTALIS_NEXUS_DEFAULT_NODE_ID");

/// Where to look first for the default service, when discovery should not be
/// relied on to find it — a container that cannot answer mDNS, mostly.
const DEFAULT_ADDRESS: Option<&str> = option_env!("PORTALIS_NEXUS_DEFAULT_ADDR");

impl NexusEndpointConfig {
    /// The service to use when nothing has been saved.
    #[must_use]
    fn default_service() -> Self {
        Self {
            node_id: normalise(DEFAULT_NODE_ID.map(str::to_owned)),
            direct_address: normalise(DEFAULT_ADDRESS.map(str::to_owned)),
        }
    }

    /// Whether this is the service the build ships with rather than a choice.
    ///
    /// Worth showing: "Portalis service" and "the one you typed in" deserve
    /// different words, and a person who has overridden the default should be
    /// able to see that they did.
    #[must_use]
    pub fn is_default_service(&self) -> bool {
        present(&self.node_id) == present(&Self::default_service().node_id)
    }
}

/// The Nexus service this build talks to.
///
/// There is nothing to load. The service is the same one for everybody and is
/// compiled in, so this is a constant read through the same validation as any
/// other endpoint rather than a setting with a history.
///
/// It used to be stored, which meant an address written once outlived every
/// later change to what the default was — a device kept dialling a port
/// nothing had listened on for months, and the only way out was a screen
/// telling people to edit something they should never have been shown.
///
/// # Errors
///
/// Returns an error when the compiled-in service is not a valid endpoint,
/// which is a build mistake rather than anything a person did.
pub fn nexus_endpoint_config() -> anyhow::Result<NexusEndpointConfig> {
    NexusEndpointConfig::default_service().normalized()
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

    /// The service is whatever this build was compiled with, on every run.
    ///
    /// Asserted against the build's own constant, so it holds both for a
    /// release that pins a service and for a checkout with none.
    #[test]
    fn the_service_is_the_one_this_build_ships_with() {
        let _temporary_state = crate::paths::redirect_to_temp();

        let service = nexus_endpoint_config().expect("a compiled-in service is valid");

        assert_eq!(service, NexusEndpointConfig::default_service());
        assert!(service.is_default_service());
        assert_eq!(
            service.node_id.is_some(),
            DEFAULT_NODE_ID.is_some_and(|id| !id.trim().is_empty()),
            "a build with a service reaches one, and a build without does not"
        );
    }

    /// Nothing a person did can leave the app dialling somewhere stale.
    ///
    /// This is the whole point of dropping the stored copy: an address saved
    /// once used to outlive every later change to the default, and the only
    /// way out was a settings screen that should not have existed.
    #[test]
    fn no_earlier_state_can_override_it() {
        let _temporary_state = crate::paths::redirect_to_temp();
        // A leftover file from a version that stored one, pointing at a port
        // nothing has listened on for a long time.
        crate::vault::Vault::named("nexus.json")
            .write(&NexusEndpointConfig {
                node_id: Some(node_id()),
                direct_address: Some("127.0.0.1:4433".to_owned()),
            })
            .expect("a stale file is written");

        let service = nexus_endpoint_config().expect("readable");

        assert_eq!(
            service,
            NexusEndpointConfig::default_service(),
            "a stored endpoint is not consulted at all"
        );
    }
}
