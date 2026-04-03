//! IP address pool — allocate and release IPs within a subnet.

use crate::error::StivaError;
use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr};

/// An IP address pool for a bridge network.
#[derive(Debug)]
pub struct IpPool {
    /// Base network address (e.g., 172.17.0.0).
    base: u32,
    /// Subnet prefix length (e.g., 16 for /16).
    prefix_len: u8,
    /// Gateway address (first usable, e.g., 172.17.0.1).
    gateway: Ipv4Addr,
    /// Currently allocated addresses.
    allocated: HashSet<Ipv4Addr>,
    /// Next candidate for allocation.
    next: u32,
}

impl IpPool {
    /// Create a new IP pool for a subnet.
    ///
    /// `subnet` is in CIDR notation (e.g., "172.17.0.0/16").
    /// The gateway is the first usable address (`.1`).
    pub fn new(subnet: &str) -> Result<Self, StivaError> {
        let (addr_str, prefix_str) = subnet
            .split_once('/')
            .ok_or_else(|| StivaError::Network(format!("invalid subnet CIDR: {subnet}")))?;

        let addr: Ipv4Addr = addr_str
            .parse()
            .map_err(|e| StivaError::Network(format!("invalid subnet address: {e}")))?;

        let prefix_len: u8 = prefix_str
            .parse()
            .map_err(|e| StivaError::Network(format!("invalid prefix length: {e}")))?;

        if prefix_len > 30 {
            return Err(StivaError::Network(format!(
                "prefix /{prefix_len} too small for a pool"
            )));
        }

        let base = u32::from(addr);
        let mask = if prefix_len == 0 {
            0
        } else {
            !((1u32 << (32 - prefix_len)) - 1)
        };
        let base = base & mask; // Ensure network-aligned.

        let gateway = Ipv4Addr::from(base + 1);

        Ok(Self {
            base,
            prefix_len,
            gateway,
            allocated: HashSet::new(),
            next: base + 2, // Start allocation from .2.
        })
    }

    /// Gateway address for this pool.
    #[inline]
    #[must_use]
    pub fn gateway(&self) -> Ipv4Addr {
        self.gateway
    }

    /// Prefix length.
    #[inline]
    #[must_use]
    pub fn prefix_len(&self) -> u8 {
        self.prefix_len
    }

    /// Subnet string in CIDR notation.
    #[inline]
    #[must_use]
    pub fn subnet(&self) -> String {
        format!("{}/{}", Ipv4Addr::from(self.base), self.prefix_len)
    }

    /// Number of allocated addresses.
    #[inline]
    #[must_use]
    pub fn allocated_count(&self) -> usize {
        self.allocated.len()
    }

    /// Maximum number of allocatable addresses (excluding network + gateway + broadcast).
    #[inline]
    fn max_hosts(&self) -> u32 {
        let total = 1u32 << (32 - self.prefix_len);
        total.saturating_sub(3) // minus network addr, gateway, broadcast
    }

    /// Broadcast address.
    #[inline]
    fn broadcast(&self) -> u32 {
        self.base + (1u32 << (32 - self.prefix_len)) - 1
    }

    /// Allocate the next available IP address.
    pub fn allocate(&mut self) -> Result<Ipv4Addr, StivaError> {
        if self.allocated.len() >= self.max_hosts() as usize {
            return Err(StivaError::Network(format!(
                "IP pool exhausted for subnet {}",
                self.subnet()
            )));
        }

        let broadcast = self.broadcast();

        // Scan from `next` for a free address.
        let start = self.next;
        loop {
            if self.next >= broadcast {
                self.next = self.base + 2; // Wrap around.
            }

            let candidate = Ipv4Addr::from(self.next);
            self.next += 1;

            // Skip gateway and already-allocated.
            if candidate == self.gateway || self.allocated.contains(&candidate) {
                // If we've wrapped around to start, pool is full.
                if self.next == start {
                    return Err(StivaError::Network("IP pool exhausted".into()));
                }
                continue;
            }

            self.allocated.insert(candidate);
            return Ok(candidate);
        }
    }

    /// Release an IP address back to the pool.
    pub fn release(&mut self, ip: &Ipv4Addr) -> bool {
        self.allocated.remove(ip)
    }
}

/// An IPv6 address pool for a bridge network.
#[derive(Debug)]
pub struct Ipv6Pool {
    /// Base network address (first 64 bits).
    base: u128,
    /// Subnet prefix length.
    prefix_len: u8,
    /// Gateway address.
    gateway: Ipv6Addr,
    /// Currently allocated addresses.
    allocated: HashSet<Ipv6Addr>,
    /// Next candidate for allocation.
    next: u128,
}

impl Ipv6Pool {
    /// Create a new IPv6 pool for a subnet (e.g., "fd00::/64").
    pub fn new(subnet: &str) -> Result<Self, StivaError> {
        let (addr_str, prefix_str) = subnet
            .split_once('/')
            .ok_or_else(|| StivaError::Network(format!("invalid IPv6 subnet CIDR: {subnet}")))?;

        let addr: Ipv6Addr = addr_str
            .parse()
            .map_err(|e| StivaError::Network(format!("invalid IPv6 address: {e}")))?;

        let prefix_len: u8 = prefix_str
            .parse()
            .map_err(|e| StivaError::Network(format!("invalid prefix length: {e}")))?;

        if prefix_len > 126 {
            return Err(StivaError::Network(format!(
                "IPv6 prefix /{prefix_len} too small for a pool"
            )));
        }

        let base = u128::from(addr);
        let mask = if prefix_len == 0 {
            0
        } else {
            !((1u128 << (128 - prefix_len)) - 1)
        };
        let base = base & mask;
        let gateway = Ipv6Addr::from(base + 1);

        Ok(Self {
            base,
            prefix_len,
            gateway,
            allocated: HashSet::new(),
            next: base + 2,
        })
    }

    /// Gateway address.
    #[inline]
    #[must_use]
    pub fn gateway(&self) -> Ipv6Addr {
        self.gateway
    }

    /// Subnet string in CIDR notation.
    #[inline]
    #[must_use]
    pub fn subnet(&self) -> String {
        format!("{}/{}", Ipv6Addr::from(self.base), self.prefix_len)
    }

    /// Allocate the next available IPv6 address.
    pub fn allocate(&mut self) -> Result<Ipv6Addr, StivaError> {
        // For /64, we have 2^64 addresses — practically unlimited.
        // Cap scan at 65534 to avoid infinite loops on exhausted small pools.
        let max_scan = 65534u128;
        let start = self.next;

        for _ in 0..max_scan {
            let candidate = Ipv6Addr::from(self.next);
            self.next += 1;

            if candidate == self.gateway || self.allocated.contains(&candidate) {
                continue;
            }

            self.allocated.insert(candidate);
            return Ok(candidate);
        }

        // Shouldn't happen for /64 pools, but handle gracefully.
        self.next = start;
        Err(StivaError::Network("IPv6 pool exhausted".into()))
    }

    /// Release an IPv6 address back to the pool.
    pub fn release(&mut self, ip: &Ipv6Addr) -> bool {
        self.allocated.remove(ip)
    }
}

/// A dual-stack IP pool supporting both IPv4 and optional IPv6.
#[derive(Debug)]
pub struct DualStackPool {
    /// IPv4 pool (always present).
    pub v4: IpPool,
    /// IPv6 pool (optional).
    pub v6: Option<Ipv6Pool>,
}

impl DualStackPool {
    /// Create a dual-stack pool with only IPv4.
    pub fn v4_only(subnet: &str) -> Result<Self, StivaError> {
        Ok(Self {
            v4: IpPool::new(subnet)?,
            v6: None,
        })
    }

    /// Create a dual-stack pool with both IPv4 and IPv6.
    pub fn dual(v4_subnet: &str, v6_subnet: &str) -> Result<Self, StivaError> {
        Ok(Self {
            v4: IpPool::new(v4_subnet)?,
            v6: Some(Ipv6Pool::new(v6_subnet)?),
        })
    }

    /// Allocate an IPv4 address.
    pub fn allocate_v4(&mut self) -> Result<Ipv4Addr, StivaError> {
        self.v4.allocate()
    }

    /// Allocate an IPv6 address (if IPv6 pool exists).
    pub fn allocate_v6(&mut self) -> Result<Option<Ipv6Addr>, StivaError> {
        match &mut self.v6 {
            Some(pool) => pool.allocate().map(Some),
            None => Ok(None),
        }
    }

    /// Allocate both addresses. Returns (v4, Option<v6>).
    pub fn allocate(&mut self) -> Result<(Ipv4Addr, Option<Ipv6Addr>), StivaError> {
        let v4 = self.v4.allocate()?;
        let v6 = self.allocate_v6()?;
        Ok((v4, v6))
    }

    /// Release both addresses.
    pub fn release(&mut self, v4: &Ipv4Addr, v6: Option<&Ipv6Addr>) {
        self.v4.release(v4);
        if let (Some(pool), Some(addr)) = (&mut self.v6, v6) {
            pool.release(addr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pool() {
        let pool = IpPool::new("172.17.0.0/16").unwrap();
        assert_eq!(pool.gateway(), "172.17.0.1".parse::<Ipv4Addr>().unwrap());
        assert_eq!(pool.prefix_len(), 16);
        assert_eq!(pool.subnet(), "172.17.0.0/16");
        assert_eq!(pool.allocated_count(), 0);
    }

    #[test]
    fn allocate_sequential() {
        let mut pool = IpPool::new("172.17.0.0/24").unwrap();
        let ip1 = pool.allocate().unwrap();
        let ip2 = pool.allocate().unwrap();
        let ip3 = pool.allocate().unwrap();
        assert_eq!(ip1, "172.17.0.2".parse::<Ipv4Addr>().unwrap());
        assert_eq!(ip2, "172.17.0.3".parse::<Ipv4Addr>().unwrap());
        assert_eq!(ip3, "172.17.0.4".parse::<Ipv4Addr>().unwrap());
        assert_eq!(pool.allocated_count(), 3);
    }

    #[test]
    fn release_and_reuse() {
        let mut pool = IpPool::new("172.17.0.0/24").unwrap();
        let ip1 = pool.allocate().unwrap();
        let _ip2 = pool.allocate().unwrap();
        assert_eq!(pool.allocated_count(), 2);

        assert!(pool.release(&ip1));
        assert_eq!(pool.allocated_count(), 1);

        // ip2 is still allocated, ip1 was released.
        // Next alloc should give ip3 (sequential), then wrap to ip1 when needed.
        let ip3 = pool.allocate().unwrap();
        assert_eq!(ip3, "172.17.0.4".parse::<Ipv4Addr>().unwrap());
    }

    #[test]
    fn release_nonexistent() {
        let mut pool = IpPool::new("172.17.0.0/24").unwrap();
        assert!(!pool.release(&"172.17.0.99".parse().unwrap()));
    }

    #[test]
    fn pool_exhaustion() {
        // /30 = 4 addresses total, minus network + gateway + broadcast = 1 usable.
        let mut pool = IpPool::new("10.0.0.0/30").unwrap();
        let ip = pool.allocate().unwrap();
        assert_eq!(ip, "10.0.0.2".parse::<Ipv4Addr>().unwrap());

        // Pool should be exhausted.
        assert!(pool.allocate().is_err());
    }

    #[test]
    fn skips_gateway() {
        // /30 has base=.0, gateway=.1, usable=.2, broadcast=.3.
        let pool = IpPool::new("10.0.0.0/30").unwrap();
        assert_eq!(pool.gateway(), "10.0.0.1".parse::<Ipv4Addr>().unwrap());
    }

    #[test]
    fn invalid_subnet() {
        assert!(IpPool::new("not-a-cidr").is_err());
        assert!(IpPool::new("172.17.0.0").is_err());
        assert!(IpPool::new("172.17.0.0/31").is_err());
        assert!(IpPool::new("172.17.0.0/32").is_err());
    }

    #[test]
    fn invalid_address() {
        assert!(IpPool::new("999.999.999.999/24").is_err());
    }

    #[test]
    fn large_pool() {
        let mut pool = IpPool::new("10.0.0.0/24").unwrap();
        // /24 = 256 addresses, minus 3 = 253 usable.
        for _ in 0..253 {
            pool.allocate().unwrap();
        }
        assert_eq!(pool.allocated_count(), 253);
        assert!(pool.allocate().is_err());
    }

    #[test]
    fn network_alignment() {
        // Non-aligned base should get aligned.
        let pool = IpPool::new("172.17.0.5/24").unwrap();
        assert_eq!(pool.subnet(), "172.17.0.0/24");
    }

    // -- IPv6 pool tests --

    #[test]
    fn ipv6_pool_new() {
        let pool = Ipv6Pool::new("fd00::/64").unwrap();
        assert_eq!(pool.gateway(), "fd00::1".parse::<Ipv6Addr>().unwrap());
        assert_eq!(pool.subnet(), "fd00::/64");
    }

    #[test]
    fn ipv6_pool_allocate() {
        let mut pool = Ipv6Pool::new("fd00::/64").unwrap();
        let ip1 = pool.allocate().unwrap();
        let ip2 = pool.allocate().unwrap();
        assert_eq!(ip1, "fd00::2".parse::<Ipv6Addr>().unwrap());
        assert_eq!(ip2, "fd00::3".parse::<Ipv6Addr>().unwrap());
    }

    #[test]
    fn ipv6_pool_release() {
        let mut pool = Ipv6Pool::new("fd00::/64").unwrap();
        let ip = pool.allocate().unwrap();
        assert!(pool.release(&ip));
        assert!(!pool.release(&"fd00::99".parse().unwrap()));
    }

    #[test]
    fn ipv6_pool_invalid() {
        assert!(Ipv6Pool::new("not-a-cidr").is_err());
        assert!(Ipv6Pool::new("fd00::").is_err()); // no prefix
        assert!(Ipv6Pool::new("fd00::/127").is_err()); // too small
    }

    // -- DualStack pool tests --

    #[test]
    fn dual_stack_v4_only() {
        let mut pool = DualStackPool::v4_only("172.17.0.0/24").unwrap();
        let (v4, v6) = pool.allocate().unwrap();
        assert_eq!(v4, "172.17.0.2".parse::<Ipv4Addr>().unwrap());
        assert!(v6.is_none());
    }

    #[test]
    fn dual_stack_both() {
        let mut pool = DualStackPool::dual("172.17.0.0/24", "fd00::/64").unwrap();
        let (v4, v6) = pool.allocate().unwrap();
        assert_eq!(v4, "172.17.0.2".parse::<Ipv4Addr>().unwrap());
        assert_eq!(v6, Some("fd00::2".parse::<Ipv6Addr>().unwrap()));
    }
}
