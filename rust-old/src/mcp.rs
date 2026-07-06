//! MCP (Model Context Protocol) tools for stiva.
//!
//! Exposes container runtime operations as MCP tools that can be discovered
//! and invoked by AI agents via daimon.

use std::collections::HashMap;
use std::sync::Arc;

use bote::{ToolAnnotations, ToolDef, ToolSchema};
use serde::{Deserialize, Serialize};

use crate::Stiva;

/// Result of an MCP tool invocation in structured MCP content format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResult {
    /// Whether the invocation succeeded.
    pub success: bool,
    /// Structured content array (MCP 2025-03-26 format).
    pub content: Vec<ContentPart>,
}

/// A typed content part in an MCP tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum ContentPart {
    /// Plain text content.
    #[serde(rename = "text")]
    Text { text: String },
    /// JSON resource content.
    #[serde(rename = "resource")]
    Resource {
        uri: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        text: String,
    },
}

impl McpResult {
    /// Create a success result with text content.
    #[must_use]
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            success: true,
            content: vec![ContentPart::Text {
                text: serde_json::to_string_pretty(&data).unwrap_or_default(),
            }],
        }
    }

    /// Create a success result with a JSON resource.
    #[must_use]
    pub fn resource(uri: &str, data: serde_json::Value) -> Self {
        Self {
            success: true,
            content: vec![ContentPart::Resource {
                uri: uri.to_string(),
                mime_type: "application/json".to_string(),
                text: serde_json::to_string_pretty(&data).unwrap_or_default(),
            }],
        }
    }

    /// Create an error result.
    #[must_use]
    pub fn err(message: &str) -> Self {
        Self {
            success: false,
            content: vec![ContentPart::Text {
                text: message.to_string(),
            }],
        }
    }
}

/// Return the list of MCP tools provided by stiva.
#[must_use]
pub fn tool_list() -> Vec<ToolDef> {
    vec![
        ToolDef::new(
            "stiva_pull",
            "Pull an OCI container image from a registry",
            ToolSchema::new(
                "object",
                HashMap::from([(
                    "image".into(),
                    serde_json::json!({"type": "string", "description": "Image reference (e.g., 'nginx:latest', 'ghcr.io/org/repo:tag')"}),
                )]),
                vec!["image".into()],
            ),
        )
        .with_annotations(ToolAnnotations::read_only()),
        ToolDef::new(
            "stiva_run",
            "Run a container from an image",
            ToolSchema::new(
                "object",
                HashMap::from([
                    (
                        "image".into(),
                        serde_json::json!({"type": "string", "description": "Image reference to run"}),
                    ),
                    (
                        "name".into(),
                        serde_json::json!({"type": "string", "description": "Container name (optional)"}),
                    ),
                    (
                        "command".into(),
                        serde_json::json!({"type": "array", "items": {"type": "string"}, "description": "Command to execute (overrides entrypoint)"}),
                    ),
                    (
                        "env".into(),
                        serde_json::json!({"type": "object", "additionalProperties": {"type": "string"}, "description": "Environment variables"}),
                    ),
                    (
                        "ports".into(),
                        serde_json::json!({"type": "array", "items": {"type": "string"}, "description": "Port mappings (e.g., '8080:80')"}),
                    ),
                    (
                        "volumes".into(),
                        serde_json::json!({"type": "array", "items": {"type": "string"}, "description": "Volume mounts (e.g., '/host:/container:ro')"}),
                    ),
                ]),
                vec!["image".into()],
            ),
        )
        .with_annotations(ToolAnnotations::destructive()),
        ToolDef::new(
            "stiva_ps",
            "List all containers",
            ToolSchema::new("object", HashMap::new(), vec![]),
        )
        .with_annotations(ToolAnnotations::read_only()),
        ToolDef::new(
            "stiva_stop",
            "Stop a running container",
            ToolSchema::new(
                "object",
                HashMap::from([(
                    "id".into(),
                    serde_json::json!({"type": "string", "description": "Container ID to stop"}),
                )]),
                vec!["id".into()],
            ),
        )
        .with_annotations(ToolAnnotations::destructive()),
        ToolDef::new(
            "stiva_ansamblu",
            "Manage multi-service deployments via ansamblu files",
            ToolSchema::new(
                "object",
                HashMap::from([
                    (
                        "action".into(),
                        serde_json::json!({"type": "string", "enum": ["up", "down"], "description": "Ansamblu action to perform"}),
                    ),
                    (
                        "file".into(),
                        serde_json::json!({"type": "string", "description": "TOML ansamblu file content"}),
                    ),
                    (
                        "session_id".into(),
                        serde_json::json!({"type": "string", "description": "Session ID (required for 'down')"}),
                    ),
                ]),
                vec!["action".into()],
            ),
        )
        .with_annotations(ToolAnnotations::destructive()),
        ToolDef::new(
            "stiva_exec",
            "Execute a command inside a running container",
            ToolSchema::new(
                "object",
                HashMap::from([
                    (
                        "id".into(),
                        serde_json::json!({"type": "string", "description": "Container ID"}),
                    ),
                    (
                        "command".into(),
                        serde_json::json!({"type": "array", "items": {"type": "string"}, "description": "Command and arguments to execute"}),
                    ),
                ]),
                vec!["id".into(), "command".into()],
            ),
        )
        .with_annotations(ToolAnnotations::destructive()),
        ToolDef::new(
            "stiva_build",
            "Build an image from a Stivafile specification",
            ToolSchema::new(
                "object",
                HashMap::from([
                    (
                        "spec".into(),
                        serde_json::json!({"type": "string", "description": "TOML build spec content"}),
                    ),
                    (
                        "context_dir".into(),
                        serde_json::json!({"type": "string", "description": "Build context directory path"}),
                    ),
                ]),
                vec!["spec".into(), "context_dir".into()],
            ),
        )
        .with_annotations(ToolAnnotations::destructive()),
        ToolDef::new(
            "stiva_push",
            "Push a local image to a registry",
            ToolSchema::new(
                "object",
                HashMap::from([
                    (
                        "image".into(),
                        serde_json::json!({"type": "string", "description": "Image ID or reference to push"}),
                    ),
                    (
                        "target".into(),
                        serde_json::json!({"type": "string", "description": "Target registry reference (optional)"}),
                    ),
                ]),
                vec!["image".into()],
            ),
        )
        .with_annotations(ToolAnnotations::destructive()),
        ToolDef::new(
            "stiva_inspect",
            "Inspect a container or image",
            ToolSchema::new(
                "object",
                HashMap::from([
                    (
                        "id".into(),
                        serde_json::json!({"type": "string", "description": "Container or image ID"}),
                    ),
                    (
                        "type".into(),
                        serde_json::json!({"type": "string", "enum": ["container", "image"], "description": "What to inspect"}),
                    ),
                ]),
                vec!["id".into(), "type".into()],
            ),
        )
        .with_annotations(ToolAnnotations::read_only()),
    ]
}

// ---------------------------------------------------------------------------
// Live tool dispatch (wired to Stiva instance)
// ---------------------------------------------------------------------------

/// Dispatch an MCP tool invocation against a live Stiva instance.
pub async fn handle_tool(stiva: &Arc<Stiva>, name: &str, params: &serde_json::Value) -> McpResult {
    match name {
        "stiva_pull" => handle_pull(stiva, params).await,
        "stiva_run" => handle_run(stiva, params).await,
        "stiva_ps" => handle_ps(stiva).await,
        "stiva_stop" => handle_stop(stiva, params).await,
        "stiva_exec" => handle_exec(stiva, params).await,
        "stiva_build" => handle_build(params).await,
        "stiva_push" => handle_push(stiva, params).await,
        "stiva_inspect" => handle_inspect(stiva, params).await,
        "stiva_ansamblu" => handle_ansamblu(params).await,
        _ => McpResult::err(&format!("unknown tool: {name}")),
    }
}

async fn handle_pull(stiva: &Arc<Stiva>, params: &serde_json::Value) -> McpResult {
    let image = match params.get("image").and_then(|v| v.as_str()) {
        Some(img) => img,
        None => return McpResult::err("missing required parameter: image"),
    };

    match stiva.pull(image).await {
        Ok(img) => McpResult::resource(
            &format!("stiva://images/{}", img.id),
            serde_json::json!({
                "id": img.id,
                "reference": img.reference.full_ref(),
                "size_bytes": img.size_bytes,
                "layers": img.layers.len(),
            }),
        ),
        Err(e) => McpResult::err(&format!("pull failed: {e}")),
    }
}

async fn handle_run(stiva: &Arc<Stiva>, params: &serde_json::Value) -> McpResult {
    let image = match params.get("image").and_then(|v| v.as_str()) {
        Some(img) => img,
        None => return McpResult::err("missing required parameter: image"),
    };

    let mut config = crate::container::ContainerConfig::default();

    if let Some(name) = params.get("name").and_then(|v| v.as_str()) {
        config.name = Some(name.to_string());
    }
    if let Some(cmd) = params.get("command").and_then(|v| v.as_array()) {
        config.command = cmd
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(ports) = params.get("ports").and_then(|v| v.as_array()) {
        config.ports = ports
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(vols) = params.get("volumes").and_then(|v| v.as_array()) {
        config.volumes = vols
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    config.detach = true;

    match stiva.run(image, config).await {
        Ok(container) => McpResult::resource(
            &format!("stiva://containers/{}", container.id),
            serde_json::json!({
                "id": container.id,
                "name": container.name,
                "state": format!("{:?}", container.state),
                "image": image,
            }),
        ),
        Err(e) => McpResult::err(&format!("run failed: {e}")),
    }
}

async fn handle_ps(stiva: &Arc<Stiva>) -> McpResult {
    match stiva.ps().await {
        Ok(containers) => {
            let list: Vec<serde_json::Value> = containers
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "name": c.name,
                        "state": format!("{:?}", c.state),
                        "image": c.config.name,
                    })
                })
                .collect();
            McpResult::ok(serde_json::json!({ "containers": list }))
        }
        Err(e) => McpResult::err(&format!("ps failed: {e}")),
    }
}

async fn handle_stop(stiva: &Arc<Stiva>, params: &serde_json::Value) -> McpResult {
    let id = match params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return McpResult::err("missing required parameter: id"),
    };

    match stiva.stop(id).await {
        Ok(()) => McpResult::ok(serde_json::json!({ "id": id, "status": "stopped" })),
        Err(e) => McpResult::err(&format!("stop failed: {e}")),
    }
}

async fn handle_exec(stiva: &Arc<Stiva>, params: &serde_json::Value) -> McpResult {
    let id = match params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return McpResult::err("missing required parameter: id"),
    };
    let command: Vec<String> = match params.get("command").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        None => return McpResult::err("missing required parameter: command"),
    };

    match stiva.exec(id, &command).await {
        Ok(result) => McpResult::ok(serde_json::json!({
            "exit_code": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "duration_ms": result.duration_ms,
        })),
        Err(e) => McpResult::err(&format!("exec failed: {e}")),
    }
}

async fn handle_build(params: &serde_json::Value) -> McpResult {
    let spec = match params.get("spec").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return McpResult::err("missing required parameter: spec"),
    };
    let context_dir = params
        .get("context_dir")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    // Build requires a running Stiva with image store; return parsed spec info.
    match crate::build::parse_build_spec(spec) {
        Ok(build_spec) => McpResult::ok(serde_json::json!({
            "action": "build",
            "base": build_spec.image.base,
            "name": build_spec.image.name,
            "tag": build_spec.image.tag,
            "steps": build_spec.steps.len(),
            "context_dir": context_dir,
            "status": "parsed"
        })),
        Err(e) => McpResult::err(&format!("build spec parse failed: {e}")),
    }
}

async fn handle_push(stiva: &Arc<Stiva>, params: &serde_json::Value) -> McpResult {
    let image = match params.get("image").and_then(|v| v.as_str()) {
        Some(i) => i,
        None => return McpResult::err("missing required parameter: image"),
    };
    let target = params.get("target").and_then(|v| v.as_str());

    match stiva.push(image, target).await {
        Ok(()) => McpResult::ok(serde_json::json!({
            "image": image,
            "target": target,
            "status": "pushed"
        })),
        Err(e) => McpResult::err(&format!("push failed: {e}")),
    }
}

async fn handle_inspect(stiva: &Arc<Stiva>, params: &serde_json::Value) -> McpResult {
    let id = match params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return McpResult::err("missing required parameter: id"),
    };
    let inspect_type = params
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("container");

    match inspect_type {
        "container" => match stiva.inspect(id).await {
            Ok(c) => McpResult::resource(
                &format!("stiva://containers/{}", c.id),
                serde_json::json!({
                    "id": c.id,
                    "name": c.name,
                    "state": format!("{:?}", c.state),
                    "created_at": c.created_at.to_rfc3339(),
                    "config": {
                        "command": c.config.command,
                        "env": c.config.env,
                        "ports": c.config.ports,
                        "volumes": c.config.volumes,
                        "detach": c.config.detach,
                    }
                }),
            ),
            Err(e) => McpResult::err(&format!("inspect failed: {e}")),
        },
        "image" => match stiva.inspect_image(id) {
            Ok(img) => McpResult::resource(
                &format!("stiva://images/{}", img.id),
                serde_json::json!({
                    "id": img.id,
                    "reference": img.reference.full_ref(),
                    "size_bytes": img.size_bytes,
                    "layers": img.layers.len(),
                    "created_at": img.created_at.to_rfc3339(),
                }),
            ),
            Err(e) => McpResult::err(&format!("inspect failed: {e}")),
        },
        _ => McpResult::err(&format!("unknown inspect type: {inspect_type}")),
    }
}

async fn handle_ansamblu(params: &serde_json::Value) -> McpResult {
    let action = match params.get("action").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => return McpResult::err("missing required parameter: action"),
    };

    match action {
        "up" => {
            let file = params.get("file").and_then(|v| v.as_str()).unwrap_or("");
            match crate::ansamblu::parse_ansamblu(file) {
                Ok(af) => McpResult::ok(serde_json::json!({
                    "action": "ansamblu_up",
                    "services": af.services.len(),
                    "status": "parsed"
                })),
                Err(e) => McpResult::err(&format!("ansamblu parse failed: {e}")),
            }
        }
        "down" => {
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            McpResult::ok(serde_json::json!({
                "action": "ansamblu_down",
                "session_id": session_id,
                "status": "queued"
            }))
        }
        _ => McpResult::err(&format!("unknown ansamblu action: {action}")),
    }
}

// ---------------------------------------------------------------------------
// MCP resources
// ---------------------------------------------------------------------------

/// List MCP resources exposed by stiva.
///
/// Resources provide read-only access to container runtime state.
pub async fn list_resources(stiva: &Arc<Stiva>) -> Vec<serde_json::Value> {
    let mut resources = Vec::new();

    // Container resources.
    if let Ok(containers) = stiva.ps().await {
        for c in &containers {
            resources.push(serde_json::json!({
                "uri": format!("stiva://containers/{}", c.id),
                "name": c.name.as_deref().unwrap_or(&c.id),
                "mimeType": "application/json",
                "description": format!("Container {} ({:?})", c.id, c.state),
            }));
        }
    }

    // Image resources.
    if let Ok(images) = stiva.images().await {
        for img in &images {
            resources.push(serde_json::json!({
                "uri": format!("stiva://images/{}", img.id),
                "name": img.reference.full_ref(),
                "mimeType": "application/json",
                "description": format!("Image {} ({} bytes, {} layers)", img.id, img.size_bytes, img.layers.len()),
            }));
        }
    }

    resources
}

/// Read a specific MCP resource by URI.
pub async fn read_resource(stiva: &Arc<Stiva>, uri: &str) -> Result<serde_json::Value, String> {
    if let Some(id) = uri.strip_prefix("stiva://containers/") {
        let container = stiva
            .inspect(id)
            .await
            .map_err(|e| format!("container not found: {e}"))?;
        Ok(serde_json::json!({
            "id": container.id,
            "name": container.name,
            "state": format!("{:?}", container.state),
            "created_at": container.created_at.to_rfc3339(),
            "config": {
                "command": container.config.command,
                "env": container.config.env,
                "ports": container.config.ports,
                "volumes": container.config.volumes,
            }
        }))
    } else if let Some(id) = uri.strip_prefix("stiva://images/") {
        let image = stiva
            .inspect_image(id)
            .map_err(|e| format!("image not found: {e}"))?;
        Ok(serde_json::json!({
            "id": image.id,
            "reference": image.reference.full_ref(),
            "size_bytes": image.size_bytes,
            "layers": image.layers.len(),
            "created_at": image.created_at.to_rfc3339(),
        }))
    } else {
        Err(format!("unknown resource URI: {uri}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_list_has_nine_tools() {
        let tools = tool_list();
        assert_eq!(tools.len(), 9);
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"stiva_pull"));
        assert!(names.contains(&"stiva_run"));
        assert!(names.contains(&"stiva_ps"));
        assert!(names.contains(&"stiva_stop"));
        assert!(names.contains(&"stiva_ansamblu"));
        assert!(names.contains(&"stiva_exec"));
        assert!(names.contains(&"stiva_build"));
        assert!(names.contains(&"stiva_push"));
        assert!(names.contains(&"stiva_inspect"));
    }

    #[test]
    fn tool_schemas_are_valid() {
        for tool in tool_list() {
            assert_eq!(tool.input_schema.schema_type, "object");
            assert!(!tool.description.is_empty());
            assert!(tool.annotations.is_some());
        }
    }

    #[test]
    fn mcp_result_ok() {
        let r = McpResult::ok(serde_json::json!({"key": "value"}));
        assert!(r.success);
        assert_eq!(r.content.len(), 1);
    }

    #[test]
    fn mcp_result_err() {
        let r = McpResult::err("something failed");
        assert!(!r.success);
        assert_eq!(r.content.len(), 1);
    }

    #[test]
    fn mcp_result_resource() {
        let r = McpResult::resource("stiva://test/1", serde_json::json!({"id": "1"}));
        assert!(r.success);
        assert_eq!(r.content.len(), 1);
        match &r.content[0] {
            ContentPart::Resource { uri, mime_type, .. } => {
                assert_eq!(uri, "stiva://test/1");
                assert_eq!(mime_type, "application/json");
            }
            _ => panic!("expected Resource content part"),
        }
    }

    #[test]
    fn mcp_result_serde() {
        let r = McpResult::ok(serde_json::json!({"test": true}));
        let json = serde_json::to_string(&r).unwrap();
        let back: McpResult = serde_json::from_str(&json).unwrap();
        assert!(back.success);
    }

    #[test]
    fn mcp_tool_serde() {
        let tool = &tool_list()[0];
        let json = serde_json::to_string(tool).unwrap();
        let back: ToolDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, tool.name);
    }

    #[test]
    fn content_part_text_serde() {
        let part = ContentPart::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"text\":\"hello\""));
    }

    #[test]
    fn content_part_resource_serde() {
        let part = ContentPart::Resource {
            uri: "stiva://containers/abc".into(),
            mime_type: "application/json".into(),
            text: "{}".into(),
        };
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"resource\""));
        assert!(json.contains("\"uri\":\"stiva://containers/abc\""));
    }
}
