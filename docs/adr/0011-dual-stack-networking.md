# ADR-0011: IPv6 dual-stack networking

## Status
Accepted

## Context
Modern container deployments increasingly require IPv6 connectivity. Some environments are IPv6-only, others need dual-stack for compatibility. Stiva's networking layer originally supported only IPv4 via `IpPool`.

## Decision
Introduce `Ipv6Pool` for IPv6 address allocation and `DualStackPool` that wraps an `IpPool` (v4) with an optional `Ipv6Pool` (v6). `ContainerNetwork` carries an optional `ipv6` field alongside the existing `ip` (v4) field.

`DualStackPool` provides two constructors:

- `v4_only(subnet)` -- backwards-compatible, no IPv6 allocation.
- `dual(v4_subnet, v6_subnet)` -- allocates both v4 and v6 addresses.

IPv6 pools require at least a /64 prefix. The `ipv6` field on `ContainerNetwork` is `Option<Ipv6Addr>`, serialized with `skip_serializing_if = "Option::is_none"` so existing state.json files remain valid.

## Consequences
- **Positive**: IPv6 is opt-in per network. Existing v4-only deployments are unaffected.
- **Positive**: `ContainerNetwork` serialization is backwards-compatible -- the `ipv6` field is absent when not set.
- **Positive**: `DualStackPool` keeps v4 and v6 allocation in sync -- a single `allocate()` call returns both addresses.
- **Negative**: IPv6 bridge interface setup and NDP proxy are not yet implemented; only pool allocation is complete.
- **Negative**: DNS injection does not yet write AAAA records for IPv6 addresses.
