# ADR-0009: MCP structured output format

## Status
Accepted

## Context
Stiva exposes 9 MCP tools for AI agent integration via daimon. The MCP 2025-03-26 specification requires tool results to return a content array with typed parts rather than flat JSON. Our previous implementation returned a single JSON blob, which forced agents to parse the entire response to extract specific resources.

## Decision
`McpResult` returns a `Vec<ContentPart>` where each part is either `ContentPart::Text` (plain text or pretty-printed JSON) or `ContentPart::Resource` (a JSON payload with a URI and MIME type). The `success` field indicates whether the tool invocation succeeded.

Text parts carry human-readable summaries. Resource parts carry structured data with URIs like `container://<id>` or `image://<ref>` that agents can dereference for direct access.

The `ContentPart` enum is `#[non_exhaustive]` and tagged via `#[serde(tag = "type")]` so new part types (e.g., binary blobs) can be added without breaking existing consumers.

## Consequences
- **Positive**: Agents can extract structured resources by URI without parsing free-form text.
- **Positive**: Resource URIs enable direct access patterns -- an agent can request `container://abc123` details without a second tool call.
- **Positive**: Aligns with MCP 2025-03-26 spec, so stiva tools work with any compliant agent framework.
- **Negative**: Existing MCP consumers that expected flat JSON need to update to the content-array format.
