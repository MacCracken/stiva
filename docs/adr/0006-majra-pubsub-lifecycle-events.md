# ADR-0006: Lifecycle events via majra pub/sub

## Status
Accepted

## Context
Consumers (daimon, sutra, monitoring) need to react to container state changes without polling. Options: callback hooks, channel-based events, or majra's pub/sub.

## Decision
Publish JSON events to majra `PubSub` on every container state change: created, started, start_failed, stopped, removed, paused, unpaused. Subscribers call `event_bus().subscribe("container.lifecycle")`.

## Consequences
- **Positive**: Decoupled — publishers don't know about subscribers.
- **Positive**: majra PubSub is already a dependency (used for health monitoring).
- **Positive**: Events are structured JSON with container_id, making filtering easy.
- **Negative**: In-process only — no network pub/sub. Daimon integration requires an HTTP bridge.
- **Negative**: No event persistence — missed events are lost. Subscribers must be active.
