# ADR-0002: TOML over YAML for ansamblu and build specs

## Status
Accepted

## Context
Docker uses YAML for Compose files and Dockerfiles for builds. YAML has implicit typing issues (the "Norway problem"), no standard schema, and ambiguous multiline strings.

## Decision
Use TOML for both ansamblu files and build specs (Stivafile). No YAML support.

## Consequences
- **Positive**: Typed, unambiguous, serde-native. No implicit type coercion.
- **Positive**: Single parser dependency (`toml` crate).
- **Positive**: Build specs are validated at parse time via serde derive.
- **Negative**: Not compatible with existing docker-compose.yml files.
- **Negative**: Less familiar to users coming from Docker ecosystem.
