//! NAT and port mapping via nein.

use crate::error::StivaError;
use nein::bridge::{BridgeConfig, PortMapping};
use nein::nat::NatRule;
use nein::rule::Protocol;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

/// A parsed port specification from container config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortSpec {
    /// Host bind address (default: 0.0.0.0).
    pub host_addr: Option<Ipv4Addr>,
    /// Port on the host.
    pub host_port: u16,
    /// Port inside the container.
    pub container_port: u16,
    /// Protocol (tcp or udp).
    pub protocol: PortProtocol,
}

/// Protocol for port mappings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PortProtocol {
    #[default]
    Tcp,
    Udp,
}

/// Parse a port specification string.
///
/// Formats:
/// - `"8080:80"` — host 8080 → container 80 (TCP)
/// - `"8080:80/tcp"` — explicit TCP
/// - `"8080:80/udp"` — UDP
/// - `"0.0.0.0:8080:80"` — explicit bind address
/// - `"127.0.0.1:8080:80/tcp"` — bind address + protocol
#[must_use = "parsing returns a new PortSpec"]
pub fn parse_port_spec(spec: &str) -> Result<PortSpec, StivaError> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(StivaError::PortMapping("empty port spec".into()));
    }

    // Split off protocol suffix.
    let (port_part, protocol) = if let Some((p, proto)) = spec.rsplit_once('/') {
        let protocol = match proto.to_lowercase().as_str() {
            "tcp" => PortProtocol::Tcp,
            "udp" => PortProtocol::Udp,
            _ => {
                return Err(StivaError::PortMapping(format!(
                    "unknown protocol: {proto}"
                )));
            }
        };
        (p, protocol)
    } else {
        (spec, PortProtocol::Tcp)
    };

    let parts: Vec<&str> = port_part.split(':').collect();

    match parts.len() {
        // host_port:container_port
        2 => {
            let host_port = parse_port(parts[0])?;
            let container_port = parse_port(parts[1])?;
            Ok(PortSpec {
                host_addr: None,
                host_port,
                container_port,
                protocol,
            })
        }
        // host_addr:host_port:container_port
        3 => {
            let host_addr: Ipv4Addr = parts[0].parse().map_err(|e| {
                StivaError::PortMapping(format!("invalid bind address '{}': {e}", parts[0]))
            })?;
            let host_port = parse_port(parts[1])?;
            let container_port = parse_port(parts[2])?;
            Ok(PortSpec {
                host_addr: Some(host_addr),
                host_port,
                container_port,
                protocol,
            })
        }
        _ => Err(StivaError::PortMapping(format!(
            "invalid port spec: {spec}"
        ))),
    }
}

fn parse_port(s: &str) -> Result<u16, StivaError> {
    s.parse::<u16>()
        .map_err(|e| StivaError::PortMapping(format!("invalid port number '{s}': {e}")))
}

/// Convert a PortSpec to a nein PortMapping.
#[must_use]
pub fn to_nein_port_mapping(spec: &PortSpec, container_ip: Ipv4Addr) -> PortMapping {
    let protocol = match spec.protocol {
        PortProtocol::Tcp => Protocol::Tcp,
        PortProtocol::Udp => Protocol::Udp,
    };

    PortMapping {
        host_port: spec.host_port,
        container_addr: container_ip.to_string(),
        container_port: spec.container_port,
        protocol,
    }
}

/// Build a masquerade NAT rule for a bridge subnet.
#[must_use]
pub fn masquerade_rule(subnet: &str, outbound_iface: &str) -> NatRule {
    nein::nat::container_masquerade(subnet, outbound_iface)
}

/// Build a DNAT (port forward) rule.
#[must_use]
pub fn port_forward_rule(host_port: u16, container_addr: &str, container_port: u16) -> NatRule {
    nein::nat::port_forward(host_port, container_addr, container_port)
}

/// Create a nein BridgeConfig.
#[must_use]
pub fn bridge_config(bridge_name: &str, subnet: &str, outbound_iface: &str) -> BridgeConfig {
    BridgeConfig::new(bridge_name, subnet, outbound_iface)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_port() {
        let spec = parse_port_spec("8080:80").unwrap();
        assert_eq!(spec.host_port, 8080);
        assert_eq!(spec.container_port, 80);
        assert_eq!(spec.protocol, PortProtocol::Tcp);
        assert!(spec.host_addr.is_none());
    }

    #[test]
    fn parse_port_with_tcp() {
        let spec = parse_port_spec("8080:80/tcp").unwrap();
        assert_eq!(spec.protocol, PortProtocol::Tcp);
    }

    #[test]
    fn parse_port_with_udp() {
        let spec = parse_port_spec("53:53/udp").unwrap();
        assert_eq!(spec.protocol, PortProtocol::Udp);
        assert_eq!(spec.host_port, 53);
        assert_eq!(spec.container_port, 53);
    }

    #[test]
    fn parse_port_with_bind_addr() {
        let spec = parse_port_spec("0.0.0.0:8080:80").unwrap();
        assert_eq!(spec.host_addr, Some("0.0.0.0".parse::<Ipv4Addr>().unwrap()));
        assert_eq!(spec.host_port, 8080);
        assert_eq!(spec.container_port, 80);
    }

    #[test]
    fn parse_port_with_localhost() {
        let spec = parse_port_spec("127.0.0.1:3000:3000/tcp").unwrap();
        assert_eq!(
            spec.host_addr,
            Some("127.0.0.1".parse::<Ipv4Addr>().unwrap())
        );
        assert_eq!(spec.protocol, PortProtocol::Tcp);
    }

    #[test]
    fn parse_port_invalid_empty() {
        assert!(parse_port_spec("").is_err());
    }

    #[test]
    fn parse_port_invalid_no_colon() {
        assert!(parse_port_spec("8080").is_err());
    }

    #[test]
    fn parse_port_invalid_protocol() {
        assert!(parse_port_spec("8080:80/sctp").is_err());
    }

    #[test]
    fn parse_port_invalid_port_number() {
        assert!(parse_port_spec("99999:80").is_err());
        assert!(parse_port_spec("8080:abc").is_err());
    }

    #[test]
    fn parse_port_invalid_bind_addr() {
        assert!(parse_port_spec("not-an-ip:8080:80").is_err());
    }

    #[test]
    fn to_nein_mapping() {
        let spec = PortSpec {
            host_addr: None,
            host_port: 8080,
            container_port: 80,
            protocol: PortProtocol::Tcp,
        };
        let mapping = to_nein_port_mapping(&spec, "172.17.0.2".parse().unwrap());
        assert_eq!(mapping.host_port, 8080);
        assert_eq!(mapping.container_port, 80);
        assert_eq!(mapping.container_addr, "172.17.0.2");
    }

    #[test]
    fn port_spec_serde() {
        let spec = PortSpec {
            host_addr: Some("127.0.0.1".parse().unwrap()),
            host_port: 443,
            container_port: 443,
            protocol: PortProtocol::Tcp,
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: PortSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn port_protocol_default() {
        assert_eq!(PortProtocol::default(), PortProtocol::Tcp);
    }

    #[test]
    fn masquerade_rule_builds() {
        let rule = masquerade_rule("172.17.0.0/16", "eth0");
        match rule {
            NatRule::Masquerade {
                source_cidr, oif, ..
            } => {
                assert_eq!(source_cidr, "172.17.0.0/16");
                assert_eq!(oif.as_deref(), Some("eth0"));
            }
            _ => panic!("expected Masquerade rule"),
        }
    }

    #[test]
    fn port_forward_rule_builds() {
        let rule = port_forward_rule(8080, "172.17.0.2", 80);
        match rule {
            NatRule::Dnat {
                dest_port, to_port, ..
            } => {
                assert_eq!(dest_port, 8080);
                assert_eq!(to_port, 80);
            }
            _ => panic!("expected Dnat rule"),
        }
    }
}
