# Networking Guide

Stiva provides bridge networking with NAT, port mapping, DNS resolution, and IPv6 dual-stack support. The network stack is built on nein (nftables).

## Bridge Networking

On startup, stiva creates a default bridge network:

- **Name**: `stiva0`
- **Subnet**: `172.17.0.0/16`
- **Driver**: Bridge

Every container connected to a bridge gets an IPv4 address from the subnet pool, a veth pair linking it to the bridge, and NAT masquerade for outbound traffic.

> ⛔ **The bridge/NAT/DNS network stack is NOT ATTACHED TO ANY CONTAINER.** Everything
> below — bridge creation, IP allocation, NAT, DNS injection, policies — is implemented
> and tested as a **library API**, and it has **no caller in `src/` at all** — not `run`,
> and not ansamblu. `network_manager_new` is invoked only from the test suite;
> `ContainerManager.network_manager` is initialised to 0 (`src/container.cyr:1051`) and
> never assigned, so the connect hook short-circuits (`:1627`). `build_sandbox`
> additionally hard-disables networking outright (`src/runtime.cyr:811`).
>
> ⚠️ Attaching the network on the `run` path — and the `-p` flag that depends on it — is
> **not currently a numbered v3.1 item**; it needs filing. (A previous revision labelled
> it "§J", which is a different item: device and accelerator passthrough.)
>
> The one network capability a container *does* get is **rootless slirp4netns/pasta**, and
> only on `run -d` — see **Rootless Networking** below.

### Library

```cyrius
var mgr = network_manager_new();   # the default bridge is created automatically
```

## Custom Networks

Create isolated networks with their own subnets:

There is no CLI verb for this — use ansamblu TOML, or the library API:

### Library

```cyrius
var mgr = network_manager_new();
network_manager_create_network(mgr, "backend", "10.10.0.0/24");
network_manager_connect_container(mgr, container_id, "backend", port_specs, rootfs);
```

Containers on different networks are isolated from each other. A container can be connected to multiple networks.

## Port Mapping

Port mapping creates nftables DNAT rules via nein, and the rules are cleaned up when the
container disconnects.

**`stiva run` has no `-p` flag** — passing one is a usage error, not a silent no-op. Port
specs reach the network manager through `network_manager_connect_container`'s `port_specs`
argument, which ansamblu supplies from a service's `ports = [...]`:

```toml
[services.web]
image = "nginx:latest"
ports = ["8080:80", "8443:443"]
```

A `-p` flag on `run` arrives with §J (v3.1), the same item that attaches the network on the
run path.

## Rootless Networking

An unprivileged container cannot create a veth pair, so rootless containers get a userspace
network stack instead — **slirp4netns** or **pasta**, spawned as a daemon alongside the
container and torn down with it.

⚠️ **Only on the `run -d` path.** `_cm_start_rootless_net` is called from inside the detach
branch (`src/container.cyr:1563`, under the `cfg.detach == 1` test at `:1537`), so a
foreground `stiva run` gets no network at all. Port forwarding goes through the helper's own control
socket rather than nftables.

```cyrius
var h = start_rootless_network(preference, pid, port_mappings, log_path);
# ... container runs ...
stop_rootless_network(h);
```

The helper binaries are looked up on `PATH`; if neither is present, rootless containers run
without outbound networking rather than failing to start.

## IPv6 Dual-Stack

Stiva supports IPv6 through `DualStackPool`, which wraps an `IpPool` (v4) and an optional `Ipv6Pool` (v6).

### Library

```cyrius
var pool = dual_stack_v4_only("172.17.0.0/24");        # IPv4 only (default)
var pool = dual_stack_dual("172.17.0.0/24", "fd00::/64");   # dual-stack

dual_stack_allocate(pool, out_v4, out_v6);
# out_v4 = 172.17.0.2, out_v6 = fd00::2
```

When dual-stack is enabled, `ContainerNetwork` carries both an `ip` (v4) and an optional `ipv6` field. IPv6 is opt-in per network -- existing v4-only networks are unaffected.

See [ADR-0011](../adr/0011-dual-stack-networking.md) for design rationale.

## Container DNS Resolution

Each container gets DNS configuration injected into its rootfs at connect time:

- `/etc/resolv.conf` -- populated with the host's DNS servers
- `/etc/hosts` -- contains the container's own hostname and IP
- `/etc/hostname` -- set to the container name or ID

### Ansamblu Service Discovery — not implemented

⛔ `DnsRegistry` (`src/network_mod.cyr:213-260`) has register / resolve / `to_hosts_entries`
built and tested, but **no caller outside the test suite**. Nothing writes sibling entries
into a container's `/etc/hosts`, because nothing constructs a `NetworkManager` on any run
path. The design below is what the library supports, not what a running ansamblu does.

```toml
# ansamblu.toml
[services.web]
image = "nginx:latest"
ports = ["8080:80"]

[services.api]
image = "myapp:latest"
env = { NGINX_HOST = "web" }
```

In this example, the `api` container can reach `web` by hostname.

## Network Policies

Network isolation is enforced at two levels:

1. **Bridge isolation** -- containers on different bridge networks cannot communicate.
2. **nftables rules** -- nein manages firewall rules for NAT, port mapping, and inter-container traffic. Rules are created on connect and removed on disconnect.

Outbound NAT masquerade uses the host's default outbound interface.

## Network Drivers

| Driver | Status | Description |
|--------|--------|-------------|
| Bridge | Stable | Default. veth pairs, IP pool, NAT. |
| Overlay | Planned | Multi-host overlay networking. |
| Macvlan | Planned | Direct attachment to host NIC. |

## Cleanup

Networks and their associated firewall rules are cleaned up when containers disconnect. Use `stiva prune` to remove stopped containers and release their network resources.
