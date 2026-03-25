//! MCP (Model Context Protocol) tools for stiva.
//!
//! Exposes container runtime operations as MCP tools that can be discovered
//! and invoked by AI agents via daimon.

use serde::{Deserialize, Serialize};

/// An MCP tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Tool name (e.g., "stiva_run").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for the input parameters.
    pub input_schema: serde_json::Value,
}

/// Result of an MCP tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResult {
    /// Whether the invocation succeeded.
    pub success: bool,
    /// Result data (on success) or error message (on failure).
    pub data: serde_json::Value,
}

impl McpResult {
    /// Create a success result.
    #[must_use]
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data,
        }
    }

    /// Create an error result.
    #[must_use]
    pub fn err(message: &str) -> Self {
        Self {
            success: false,
            data: serde_json::json!({ "error": message }),
        }
    }
}

/// Return the list of MCP tools provided by stiva.
#[must_use]
pub fn tool_list() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "stiva_pull".into(),
            description: "Pull an OCI container image from a registry".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "image": {
                        "type": "string",
                        "description": "Image reference (e.g., 'nginx:latest', 'ghcr.io/org/repo:tag')"
                    }
                },
                "required": ["image"]
            }),
        },
        McpTool {
            name: "stiva_run".into(),
            description: "Run a container from an image".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "image": {
                        "type": "string",
                        "description": "Image reference to run"
                    },
                    "name": {
                        "type": "string",
                        "description": "Container name (optional)"
                    },
                    "command": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Command to execute (overrides entrypoint)"
                    },
                    "env": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Environment variables"
                    },
                    "ports": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Port mappings (e.g., '8080:80')"
                    },
                    "volumes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Volume mounts (e.g., '/host:/container:ro')"
                    }
                },
                "required": ["image"]
            }),
        },
        McpTool {
            name: "stiva_ps".into(),
            description: "List all containers".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpTool {
            name: "stiva_stop".into(),
            description: "Stop a running container".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Container ID to stop"
                    }
                },
                "required": ["id"]
            }),
        },
        McpTool {
            name: "stiva_compose".into(),
            description: "Manage multi-container deployments via compose files".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["up", "down"],
                        "description": "Compose action to perform"
                    },
                    "file": {
                        "type": "string",
                        "description": "TOML compose file content"
                    },
                    "session_id": {
                        "type": "string",
                        "description": "Session ID (required for 'down')"
                    }
                },
                "required": ["action"]
            }),
        },
        McpTool {
            name: "stiva_exec".into(),
            description: "Execute a command inside a running container".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Container ID"
                    },
                    "command": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Command and arguments to execute"
                    }
                },
                "required": ["id", "command"]
            }),
        },
        McpTool {
            name: "stiva_build".into(),
            description: "Build an image from a Stivafile.toml specification".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "spec": {
                        "type": "string",
                        "description": "TOML build spec content"
                    },
                    "context_dir": {
                        "type": "string",
                        "description": "Build context directory path"
                    }
                },
                "required": ["spec", "context_dir"]
            }),
        },
        McpTool {
            name: "stiva_push".into(),
            description: "Push a local image to a registry".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "image": {
                        "type": "string",
                        "description": "Image ID or reference to push"
                    },
                    "target": {
                        "type": "string",
                        "description": "Target registry reference (optional)"
                    }
                },
                "required": ["image"]
            }),
        },
        McpTool {
            name: "stiva_inspect".into(),
            description: "Inspect a container or image".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Container or image ID"
                    },
                    "type": {
                        "type": "string",
                        "enum": ["container", "image"],
                        "description": "What to inspect"
                    }
                },
                "required": ["id", "type"]
            }),
        },
    ]
}

/// Dispatch an MCP tool invocation.
///
/// This is the entry point for MCP tool handling. In production, the `stiva`
/// parameter would be a reference to the running Stiva instance. For now,
/// this returns structured results that describe what would happen.
pub async fn handle_tool(name: &str, params: &serde_json::Value) -> McpResult {
    match name {
        "stiva_pull" => handle_pull(params).await,
        "stiva_run" => handle_run(params).await,
        "stiva_ps" => handle_ps(params).await,
        "stiva_stop" => handle_stop(params).await,
        "stiva_compose" => handle_compose(params).await,
        "stiva_exec" => handle_exec(params).await,
        "stiva_build" => handle_build(params).await,
        "stiva_push" => handle_push(params).await,
        "stiva_inspect" => handle_inspect(params).await,
        _ => McpResult::err(&format!("unknown tool: {name}")),
    }
}

async fn handle_pull(params: &serde_json::Value) -> McpResult {
    let image = match params.get("image").and_then(|v| v.as_str()) {
        Some(img) => img,
        None => return McpResult::err("missing required parameter: image"),
    };

    // In production: stiva.pull(image).await
    McpResult::ok(serde_json::json!({
        "action": "pull",
        "image": image,
        "status": "queued"
    }))
}

async fn handle_run(params: &serde_json::Value) -> McpResult {
    let image = match params.get("image").and_then(|v| v.as_str()) {
        Some(img) => img,
        None => return McpResult::err("missing required parameter: image"),
    };

    let name = params.get("name").and_then(|v| v.as_str());

    McpResult::ok(serde_json::json!({
        "action": "run",
        "image": image,
        "name": name,
        "status": "queued"
    }))
}

async fn handle_ps(_params: &serde_json::Value) -> McpResult {
    // In production: stiva.ps().await → serialize containers
    McpResult::ok(serde_json::json!({
        "action": "ps",
        "containers": []
    }))
}

async fn handle_stop(params: &serde_json::Value) -> McpResult {
    let id = match params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return McpResult::err("missing required parameter: id"),
    };

    McpResult::ok(serde_json::json!({
        "action": "stop",
        "id": id,
        "status": "queued"
    }))
}

async fn handle_compose(params: &serde_json::Value) -> McpResult {
    let action = match params.get("action").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => return McpResult::err("missing required parameter: action"),
    };

    match action {
        "up" => {
            let file = params.get("file").and_then(|v| v.as_str()).unwrap_or("");
            McpResult::ok(serde_json::json!({
                "action": "compose_up",
                "file_len": file.len(),
                "status": "queued"
            }))
        }
        "down" => {
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            McpResult::ok(serde_json::json!({
                "action": "compose_down",
                "session_id": session_id,
                "status": "queued"
            }))
        }
        _ => McpResult::err(&format!("unknown compose action: {action}")),
    }
}

async fn handle_exec(params: &serde_json::Value) -> McpResult {
    let id = match params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return McpResult::err("missing required parameter: id"),
    };
    let command = params.get("command").and_then(|v| v.as_array());
    if command.is_none() {
        return McpResult::err("missing required parameter: command");
    }

    McpResult::ok(serde_json::json!({
        "action": "exec",
        "id": id,
        "status": "queued"
    }))
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

    McpResult::ok(serde_json::json!({
        "action": "build",
        "spec_len": spec.len(),
        "context_dir": context_dir,
        "status": "queued"
    }))
}

async fn handle_push(params: &serde_json::Value) -> McpResult {
    let image = match params.get("image").and_then(|v| v.as_str()) {
        Some(i) => i,
        None => return McpResult::err("missing required parameter: image"),
    };
    let target = params.get("target").and_then(|v| v.as_str());

    McpResult::ok(serde_json::json!({
        "action": "push",
        "image": image,
        "target": target,
        "status": "queued"
    }))
}

async fn handle_inspect(params: &serde_json::Value) -> McpResult {
    let id = match params.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return McpResult::err("missing required parameter: id"),
    };
    let inspect_type = params
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("container");

    McpResult::ok(serde_json::json!({
        "action": "inspect",
        "id": id,
        "type": inspect_type,
        "status": "queued"
    }))
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
        assert!(names.contains(&"stiva_compose"));
        assert!(names.contains(&"stiva_exec"));
        assert!(names.contains(&"stiva_build"));
        assert!(names.contains(&"stiva_push"));
        assert!(names.contains(&"stiva_inspect"));
    }

    #[test]
    fn tool_schemas_are_valid_json() {
        for tool in tool_list() {
            assert!(tool.input_schema.is_object());
            assert!(tool.input_schema.get("type").is_some());
        }
    }

    #[test]
    fn mcp_result_ok() {
        let r = McpResult::ok(serde_json::json!({"key": "value"}));
        assert!(r.success);
        assert_eq!(r.data["key"], "value");
    }

    #[test]
    fn mcp_result_err() {
        let r = McpResult::err("something failed");
        assert!(!r.success);
        assert_eq!(r.data["error"], "something failed");
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
        let back: McpTool = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, tool.name);
    }

    #[tokio::test]
    async fn handle_pull_valid() {
        let params = serde_json::json!({"image": "nginx:latest"});
        let result = handle_tool("stiva_pull", &params).await;
        assert!(result.success);
        assert_eq!(result.data["image"], "nginx:latest");
    }

    #[tokio::test]
    async fn handle_pull_missing_image() {
        let params = serde_json::json!({});
        let result = handle_tool("stiva_pull", &params).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn handle_run_valid() {
        let params = serde_json::json!({
            "image": "alpine",
            "name": "test",
            "ports": ["8080:80"]
        });
        let result = handle_tool("stiva_run", &params).await;
        assert!(result.success);
        assert_eq!(result.data["image"], "alpine");
    }

    #[tokio::test]
    async fn handle_ps() {
        let params = serde_json::json!({});
        let result = handle_tool("stiva_ps", &params).await;
        assert!(result.success);
    }

    #[tokio::test]
    async fn handle_stop_valid() {
        let params = serde_json::json!({"id": "abc123"});
        let result = handle_tool("stiva_stop", &params).await;
        assert!(result.success);
        assert_eq!(result.data["id"], "abc123");
    }

    #[tokio::test]
    async fn handle_stop_missing_id() {
        let params = serde_json::json!({});
        let result = handle_tool("stiva_stop", &params).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn handle_compose_up() {
        let params =
            serde_json::json!({"action": "up", "file": "[services.web]\nimage = \"nginx\""});
        let result = handle_tool("stiva_compose", &params).await;
        assert!(result.success);
        assert_eq!(result.data["action"], "compose_up");
    }

    #[tokio::test]
    async fn handle_compose_down() {
        let params = serde_json::json!({"action": "down", "session_id": "sess-123"});
        let result = handle_tool("stiva_compose", &params).await;
        assert!(result.success);
        assert_eq!(result.data["session_id"], "sess-123");
    }

    #[tokio::test]
    async fn handle_compose_invalid_action() {
        let params = serde_json::json!({"action": "restart"});
        let result = handle_tool("stiva_compose", &params).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn handle_unknown_tool() {
        let params = serde_json::json!({});
        let result = handle_tool("nonexistent_tool", &params).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn handle_exec_valid() {
        let params = serde_json::json!({"id": "abc", "command": ["ls"]});
        let result = handle_tool("stiva_exec", &params).await;
        assert!(result.success);
        assert_eq!(result.data["action"], "exec");
    }

    #[tokio::test]
    async fn handle_exec_missing_id() {
        let params = serde_json::json!({"command": ["ls"]});
        let result = handle_tool("stiva_exec", &params).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn handle_build_valid() {
        let params = serde_json::json!({"spec": "[image]\nbase=\"alpine\"\nname=\"x\"", "context_dir": "/tmp"});
        let result = handle_tool("stiva_build", &params).await;
        assert!(result.success);
        assert_eq!(result.data["action"], "build");
    }

    #[tokio::test]
    async fn handle_push_valid() {
        let params = serde_json::json!({"image": "nginx:latest"});
        let result = handle_tool("stiva_push", &params).await;
        assert!(result.success);
        assert_eq!(result.data["image"], "nginx:latest");
    }

    #[tokio::test]
    async fn handle_inspect_valid() {
        let params = serde_json::json!({"id": "abc", "type": "container"});
        let result = handle_tool("stiva_inspect", &params).await;
        assert!(result.success);
        assert_eq!(result.data["type"], "container");
    }
}
