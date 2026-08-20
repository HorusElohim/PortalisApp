//! Small top-level FRB-facing functions that don't belong to any specific
//! bridged module. Exists so `flutter_rust_bridge_codegen`'s `--rust-input`
//! can list explicit module paths (`crate::bridge,crate::torrent`, see
//! `tool/frb_build.sh`) instead of the bare `crate` wildcard — which walks
//! every `mod` declaration in the crate regardless of visibility and would
//! sweep up internal-only modules like `domain` too and fail to compile
//! (private fields it assumes are bridgeable). See rust/backend/README.md's
//! "Flutter boundary API".

use std::net::SocketAddr;
use std::str::FromStr;

use flutter_rust_bridge::frb;
use crate::substrate::PeerHints;
use crate::torrent::native::peer_hints_from_source;

// Keep web simple by making this a synchronous, non-threaded function.
// FRB will generate a sync binding that avoids web worker/threadpool usage.
#[frb(sync)]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Create a new PeerHints instance from a list of IP:port strings.
// 
/// # Arguments
/// * `peers` - A list of peer addresses in "ip:port" format
/// 
/// # Returns
/// * JSON string containing success status and any error message
/// 
/// # Example
/// ```javascript
/// const result = PeerHints_create(["192.168.1.100:6881", "192.168.1.101:6881"]);
// ```
// 
/// Returns: {success: true, hints: "serialized_peer_hints_data"} or 
///          {success: false, error: "error message"}
#[frb(sync)]
pub fn peer_hints_create(peers: Vec<String>) -> String {
    let socket_addrs: Result<Vec<SocketAddr>, _> = peers
        .iter()
        .map(|peer| SocketAddr::from_str(peer))
        .collect();
    
    match socket_addrs {
        Ok(addrs) => {
            match PeerHints::new(addrs) {
                Ok(hints) => {
                    // For now, we'll return a simple representation
                    // In a full implementation, we'd serialize the PeerHints
                    // For MVP, we'll just confirm creation worked
                    format!(r#"{{"success":true, "count":{}}}"#, hints.as_slice().len())
                }
                Err(e) => {
                    format!(r#"{{"success":false, "error":"{}"}}"#, e.to_string())
                }
            }
        }
        Err(e) => {
            format!(r#"{{"success":false, "error":"Invalid address format: {}"}}"#, e.to_string())
        }
    }
}

/// Parse peer hints from a magnet URI.
// 
/// # Arguments
/// * `magnet` - A magnet URI that may contain x.pe parameters
/// 
/// # Returns
/// * JSON string containing success status and peer count
#[frb(sync)]
pub fn peer_hints_from_magnet(magnet: String) -> String {
    match peer_hints_from_source(&magnet) {
        Ok(hints) => {
            format!(r#"{{"success":true, "count":{}}}"#, hints.as_slice().len())
        }
        Err(e) => {
            format!(r#"{{"success":false, "error":"{}"}}"#, e.to_string())
        }
    }
}

/// Validate if a string is a valid IP:port address.
// 
/// # Arguments
/// * `address` - A string in "ip:port" format
/// 
/// # Returns
/// * JSON string with validation result
#[frb(sync)]
pub fn peer_hints_validate_address(address: String) -> String {
    match SocketAddr::from_str(&address) {
        Ok(addr) => {
            // Additional validation: reject unspecified IPs and port 0
            if addr.ip().is_unspecified() {
                format!(r#"{{"success":false, "error":"Unspecified IP address not allowed"}}"#)
            } else if addr.port() == 0 {
                format!(r#"{{"success":false, "error":"Port 0 not allowed"}}"#)
            } else {
                format!(r#"{{"success":true, "address":"{}"}}"#, addr.to_string())
            }
        }
        Err(e) => {
            format!(r#"{{"success":false, "error":"{}"}}"#, e.to_string())
        }
    }
}


/// Discover local network peers on the standard BitTorrent port (6881).
/// 
/// This function scans network interfaces and returns PeerHints containing
/// IPv4 addresses of local network interfaces.
// 
/// # Returns
/// * JSON string with success status and peer count
/// 
/// # Example
/// ```javascript
/// const result = PeerHints_discoverLocal();
//  /// Returns: {success: true, count: 2} if two local interfaces found
///  ```
#[frb(sync)]
pub fn peer_hints_discover_local() -> String {
    let hints = crate::substrate::lan_discovery::discover_local_peers();
    format!(r#"{{"success":true, "count":{}}}"#, hints.as_slice().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_version_matches_crate_metadata() {
        assert_eq!(get_version(), env!("CARGO_PKG_VERSION"));
    }
    
    #[test]
    fn test_peer_hints_create_valid() {
        let result = peer_hints_create(vec!["192.168.1.100:6881".to_string()]);
        assert!(result.contains(r#""success":true"#));
        assert!(result.contains(r#""count":1"#));
    }
    
    #[test]
    fn test_peer_hints_create_invalid_ip() {
        let result = peer_hints_create(vec!["999.999.999.999:6881".to_string()]);
        assert!(result.contains(r#""success":false"#));
    }
    
    #[test]
    fn test_peer_hints_from_magnet() {
        let magnet = "magnet:?xt=urn:btih:abcdefghijklmnop&x.pe=192.168.1.100:6881";
        let result = peer_hints_from_magnet(magnet.to_string());
        assert!(result.contains(r#""success":true"#));
        assert!(result.contains(r#""count":1"#));
    }
    
    #[test]
    fn test_validate_address() {
        let result = peer_hints_validate_address("192.168.1.100:6881".to_string());
        assert!(result.contains(r#""success":true"#));
        
        let result2 = peer_hints_validate_address("0.0.0.0:6881".to_string());
        assert!(result2.contains(r#""success":false"#));
        assert!(result2.contains("Unspecified IP"));
    }
}
