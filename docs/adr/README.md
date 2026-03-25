# Architecture Decision Records

| ADR | Decision |
|-----|----------|
| [0001](0001-kavach-sandbox-abstraction.md) | Use kavach for sandbox abstraction |
| [0002](0002-toml-over-yaml.md) | TOML over YAML for compose and build specs |
| [0003](0003-daemon-vs-oneshot-model.md) | Dual execution model — one-shot and daemon |
| [0004](0004-persistent-container-state.md) | Persistent container state via state.json |
| [0005](0005-oci-fallback-backend-selection.md) | OCI → Process backend fallback (not auto-best) |
| [0006](0006-majra-pubsub-lifecycle-events.md) | Lifecycle events via majra pub/sub |
| [0007](0007-cgroups-v2-only.md) | Cgroups v2 only (no v1 support) |
| [0008](0008-library-first-with-cli.md) | Library-first design with thin CLI |

## Adding ADRs

New ADRs should follow the template: Status, Context, Decision, Consequences. Number sequentially.
