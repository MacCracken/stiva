# Networking Guide

Stiva provides bridge networking with NAT, port mapping, DNS resolution, and IPv6 dual-stack support. The network stack is built on nein (nftables).

## Bridge Networking

On startup, stiva creates a default bridge network:

- **Name**: `stiva0`
- **Subnet**: `172.17.0.0/16`
- **Driver**: Bridge

Every container connected to a bridge gets an IPv4 address from the subnet pool, a veth pair linking it to the bridge, and NAT masquerade for outbound traffic.

```bash
stiva run -d nginx:latest
# Container receives 172.17.0.2, routed through stiva0
```

### Library

```rust
use stiva::network::NetworkManager;

let mut mgr = NetworkManager::new()?;
// Default bridge is created automatically
```

## Custom Networks

Create isolated networks with their own subnets:

```bash
# Not yet a CLI command -- use the library or ansamblu TOML
```

### Library

```rust
let mut mgr = NetworkManager::new()?;
mgr.create_network("backend", "10.10.0.0/24")?;
mgr.connect("container-id", "backend", "/path/to/rootfs")?;
```

Containers on different networks are isolated from each other. A container can be connected to multiple networks.

## Port Mapping

Forward host ports to container ports using `-p`:

```bash
stiva run -d -p 8080:80 nginx:latest         # single port
stiva run -d -p 8080:80 -p 8443:443 myapp    # multiple ports
```

Port mapping creates nftables DNAT rules via nein. The rules are cleaned up when the container stops.

## IPv6 Dual-Stack

Stiva supports IPv6 through `DualStackPool`, which wraps an `IpPool` (v4) and an optional `Ipv6Pool` (v6).

### Library

```rust
use stiva::network::pool::DualStackPool;

// IPv4 only (default)
let mut pool = DualStackPool::v4_only("172.17.0.0/24")?;

// Dual-stack
let mut pool = DualStackPool::dual("172.17.0.0/24", "fd00::/64")?;
let (v4, v6) = pool.allocate()?;
// v4 = 172.17.0.2, v6 = Some(fd00::2)
```

When dual-stack is enabled, `ContainerNetwork` carries both an `ip` (v4) and an optional `ipv6` field. IPv6 is opt-in per network -- existing v4-only networks are unaffected.

See [ADR-0011](../adr/0011-dual-stack-networking.md) for design rationale.

## Container DNS Resolution

Each container gets DNS configuration injected into its rootfs at connect time:

- `/etc/resolv.conf` -- populated with the host's DNS servers
- `/etc/hosts` -- contains the container's own hostname and IP
- `/etc/hostname` -- set to the container name or ID

### Ansamblu Service Discovery

Within an ansamblu session, containers can resolve each other by service name. The `DnsRegistry` maintains a mapping of container names to IPs. When containers are connected to the same network, their `/etc/hosts` entries include sibling services.

```toml
# ansamblu.toml
[services.web]
image = "nginx:latest"
ports = ["8080:80"]

[services.api]
image = "myapp:latest"
environment = { NGINX_HOST = "web" }
```

In this example, the `api` container can reach `web` by hostname.

## Network Policies

Network isolation is enforced at two levels:

1. **Bridge isolation** -- containers on different bridge networks cannot communicate.
2. **nftables rules** -- nein manages firewall rules for NAT, port mapping, and inter-container traffic. Rules are created on connect and removed on disconnect.

Outbound NAT masquerade uses the host's default outbound interface (configurable via `NetworkManager::with_outbound`).

## Network Drivers

| Driver | Status | Description |
|--------|--------|-------------|
| Bridge | Stable | Default. veth pairs, IP pool, NAT. |
| Overlay | Planned | Multi-host overlay networking. |
| Macvlan | Planned | Direct attachment to host NIC. |

## Cleanup

Networks and their associated firewall rules are cleaned up when containers disconnect. Use `stiva prune` to remove stopped containers and release their network resources.
