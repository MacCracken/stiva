//! YAML → TOML conversion for Docker Compose and Dockerfile migration.
//!
//! Helps users migrate from Docker's YAML-based configuration to stiva's
//! TOML-based format.

use crate::error::StivaError;
use serde_json::Value;
use std::fmt::Write;
use tracing::info;

/// Convert a docker-compose YAML string to stiva compose TOML.
///
/// Handles the common docker-compose.yml fields:
/// - `services` with `image`, `command`, `environment`, `ports`, `volumes`, `depends_on`
/// - `networks` and `volumes` top-level sections
#[must_use = "returns the converted TOML string"]
pub fn compose_yaml_to_toml(yaml: &str) -> Result<String, StivaError> {
    info!("converting docker-compose YAML to stiva TOML");

    let doc: Value = serde_yaml::from_str(yaml)
        .map_err(|e| StivaError::InvalidState(format!("invalid YAML: {e}")))?;

    let mut out = String::new();

    // Convert services.
    if let Some(services) = doc.get("services").and_then(|s| s.as_object()) {
        for (name, svc) in services {
            writeln!(out, "[services.{name}]").unwrap();

            if let Some(image) = svc.get("image").and_then(|v| v.as_str()) {
                writeln!(out, "image = \"{image}\"").unwrap();
            }

            // command — string or array.
            if let Some(cmd) = svc.get("command") {
                if let Some(s) = cmd.as_str() {
                    // Split string command into array.
                    let parts: Vec<&str> = s.split_whitespace().collect();
                    let arr: Vec<String> = parts.iter().map(|p| format!("\"{p}\"")).collect();
                    writeln!(out, "command = [{}]", arr.join(", ")).unwrap();
                } else if let Some(arr) = cmd.as_array() {
                    let items: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| format!("\"{s}\""))
                        .collect();
                    writeln!(out, "command = [{}]", items.join(", ")).unwrap();
                }
            }

            // environment — object or array.
            if let Some(env) = svc.get("environment") {
                if let Some(obj) = env.as_object() {
                    let pairs: Vec<String> = obj
                        .iter()
                        .map(|(k, v)| {
                            let val = match v.as_str() {
                                Some(s) => s.to_string(),
                                None => v.to_string(),
                            };
                            format!("{k} = \"{val}\"")
                        })
                        .collect();
                    if !pairs.is_empty() {
                        let mut env_line = String::from("env = { ");
                        env_line.push_str(&pairs.join(", "));
                        env_line.push_str(" }");
                        writeln!(out, "{env_line}").unwrap();
                    }
                } else if let Some(arr) = env.as_array() {
                    let mut env_parts = Vec::new();
                    for item in arr {
                        if let Some(s) = item.as_str()
                            && let Some((k, v)) = s.split_once('=')
                        {
                            env_parts.push(format!("{k} = \"{v}\""));
                        }
                    }
                    if !env_parts.is_empty() {
                        let mut env_line = String::from("env = { ");
                        env_line.push_str(&env_parts.join(", "));
                        env_line.push_str(" }");
                        writeln!(out, "{env_line}").unwrap();
                    }
                }
            }

            // ports.
            if let Some(ports) = svc.get("ports").and_then(|v| v.as_array()) {
                let items: Vec<String> = ports
                    .iter()
                    .filter_map(|v| v.as_str().or_else(|| v.as_i64().map(|_| "")))
                    .filter(|s| !s.is_empty())
                    .map(|s| format!("\"{s}\""))
                    .collect();
                if !items.is_empty() {
                    writeln!(out, "ports = [{}]", items.join(", ")).unwrap();
                }
            }

            // volumes.
            if let Some(vols) = svc.get("volumes").and_then(|v| v.as_array()) {
                let items: Vec<String> = vols
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| format!("\"{s}\""))
                    .collect();
                if !items.is_empty() {
                    writeln!(out, "volumes = [{}]", items.join(", ")).unwrap();
                }
            }

            // depends_on — array or object.
            if let Some(deps) = svc.get("depends_on") {
                if let Some(arr) = deps.as_array() {
                    let items: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| format!("\"{s}\""))
                        .collect();
                    if !items.is_empty() {
                        writeln!(out, "depends_on = [{}]", items.join(", ")).unwrap();
                    }
                } else if let Some(obj) = deps.as_object() {
                    let items: Vec<String> = obj.keys().map(|k| format!("\"{k}\"")).collect();
                    if !items.is_empty() {
                        writeln!(out, "depends_on = [{}]", items.join(", ")).unwrap();
                    }
                }
            }

            // restart.
            if let Some(restart) = svc.get("restart").and_then(|v| v.as_str()) {
                writeln!(out, "restart = \"{restart}\"").unwrap();
            }

            writeln!(out).unwrap();
        }
    }

    // Convert top-level networks.
    if let Some(networks) = doc.get("networks").and_then(|n| n.as_object()) {
        for (name, net) in networks {
            writeln!(out, "[networks.{name}]").unwrap();
            if let Some(driver) = net.get("driver").and_then(|v| v.as_str()) {
                writeln!(out, "driver = \"{driver}\"").unwrap();
            }
            if let Some(subnet) = net
                .get("ipam")
                .and_then(|i| i.get("config"))
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|e| e.get("subnet"))
                .and_then(|s| s.as_str())
            {
                writeln!(out, "subnet = \"{subnet}\"").unwrap();
            }
            writeln!(out).unwrap();
        }
    }

    // Convert top-level volumes.
    if let Some(volumes) = doc.get("volumes").and_then(|v| v.as_object()) {
        for (name, vol) in volumes {
            writeln!(out, "[volumes.{name}]").unwrap();
            if let Some(driver) = vol.get("driver").and_then(|v| v.as_str()) {
                writeln!(out, "driver = \"{driver}\"").unwrap();
            }
            writeln!(out).unwrap();
        }
    }

    info!(output_len = out.len(), "conversion complete");
    Ok(out)
}

/// Convert a Dockerfile to a Stivafile build spec.
///
/// Handles common Dockerfile instructions: FROM, RUN, COPY, ENV, WORKDIR,
/// EXPOSE, ENTRYPOINT, USER, LABEL.
#[must_use = "returns the converted TOML string"]
pub fn dockerfile_to_toml(dockerfile: &str) -> Result<String, StivaError> {
    info!("converting Dockerfile to Stivafile");

    let mut base = String::new();
    let mut steps = Vec::new();
    let mut entrypoint = Vec::new();
    let mut expose: Vec<u16> = Vec::new();
    let mut user = String::new();
    let mut workdir_config = String::new();

    for line in dockerfile.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Handle line continuations.
        let line = line.trim_end_matches('\\').trim();

        if let Some(rest) = line.strip_prefix("FROM ") {
            base = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("RUN ") {
            steps.push(format!(
                "[[steps]]\ntype = \"run\"\ncommand = [\"/bin/sh\", \"-c\", \"{}\"]",
                rest.replace('\"', "\\\"")
            ));
        } else if let Some(rest) = line.strip_prefix("COPY ") {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if parts.len() == 2 {
                steps.push(format!(
                    "[[steps]]\ntype = \"copy\"\nsource = \"{}\"\ndestination = \"{}\"",
                    parts[0], parts[1]
                ));
            }
        } else if let Some(rest) = line.strip_prefix("ENV ") {
            if let Some((k, v)) = rest.split_once('=') {
                let v = v.trim().trim_matches('"');
                steps.push(format!(
                    "[[steps]]\ntype = \"env\"\nkey = \"{}\"\nvalue = \"{}\"",
                    k.trim(),
                    v
                ));
            } else if let Some((k, v)) = rest.split_once(' ') {
                steps.push(format!(
                    "[[steps]]\ntype = \"env\"\nkey = \"{}\"\nvalue = \"{}\"",
                    k.trim(),
                    v.trim()
                ));
            }
        } else if let Some(rest) = line.strip_prefix("WORKDIR ") {
            steps.push(format!(
                "[[steps]]\ntype = \"workdir\"\npath = \"{}\"",
                rest.trim()
            ));
            workdir_config = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("LABEL ") {
            if let Some((k, v)) = rest.split_once('=') {
                let v = v.trim().trim_matches('"');
                steps.push(format!(
                    "[[steps]]\ntype = \"label\"\nkey = \"{}\"\nvalue = \"{}\"",
                    k.trim(),
                    v
                ));
            }
        } else if let Some(rest) = line.strip_prefix("EXPOSE ") {
            for port_str in rest.split_whitespace() {
                // Strip protocol suffix if present.
                let port_str = port_str.split('/').next().unwrap_or(port_str);
                if let Ok(port) = port_str.parse::<u16>() {
                    expose.push(port);
                }
            }
        } else if let Some(rest) = line.strip_prefix("ENTRYPOINT ") {
            // JSON array or shell form.
            let rest = rest.trim();
            if rest.starts_with('[') {
                if let Ok(arr) = serde_json::from_str::<Vec<String>>(rest) {
                    entrypoint = arr;
                }
            } else {
                entrypoint = vec!["/bin/sh".into(), "-c".into(), rest.to_string()];
            }
        } else if let Some(rest) = line.strip_prefix("USER ") {
            user = rest.trim().to_string();
        }
        // CMD, ADD, ARG, SHELL, etc. — skip or log.
    }

    if base.is_empty() {
        return Err(StivaError::InvalidState(
            "Dockerfile has no FROM instruction".into(),
        ));
    }

    let mut out = String::new();
    writeln!(out, "[image]").unwrap();
    writeln!(out, "base = \"{base}\"").unwrap();
    writeln!(out, "name = \"converted\"").unwrap();
    writeln!(out, "tag = \"latest\"").unwrap();
    writeln!(out).unwrap();

    for step in &steps {
        writeln!(out, "{step}").unwrap();
        writeln!(out).unwrap();
    }

    writeln!(out, "[config]").unwrap();
    if !entrypoint.is_empty() {
        let items: Vec<String> = entrypoint.iter().map(|s| format!("\"{s}\"")).collect();
        writeln!(out, "entrypoint = [{}]", items.join(", ")).unwrap();
    }
    if !expose.is_empty() {
        let items: Vec<String> = expose.iter().map(|p| p.to_string()).collect();
        writeln!(out, "expose = [{}]", items.join(", ")).unwrap();
    }
    if !user.is_empty() {
        writeln!(out, "user = \"{user}\"").unwrap();
    }
    if !workdir_config.is_empty() {
        writeln!(out, "workdir = \"{workdir_config}\"").unwrap();
    }

    info!(output_len = out.len(), "Dockerfile conversion complete");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_basic() {
        let yaml = r#"
services:
  web:
    image: nginx:latest
    ports:
      - "8080:80"
  db:
    image: postgres:16
    environment:
      POSTGRES_PASSWORD: secret
    volumes:
      - pgdata:/var/lib/postgresql/data

volumes:
  pgdata:
"#;
        let toml = compose_yaml_to_toml(yaml).unwrap();
        assert!(toml.contains("[services.web]"));
        assert!(toml.contains("image = \"nginx:latest\""));
        assert!(toml.contains("\"8080:80\""));
        assert!(toml.contains("[services.db]"));
        assert!(toml.contains("POSTGRES_PASSWORD"));
        assert!(toml.contains("[volumes.pgdata]"));
    }

    #[test]
    fn compose_depends_on() {
        let yaml = r#"
services:
  api:
    image: app:latest
    depends_on:
      - db
      - cache
  db:
    image: postgres
  cache:
    image: redis
"#;
        let toml = compose_yaml_to_toml(yaml).unwrap();
        assert!(toml.contains("depends_on = [\"db\", \"cache\"]"));
    }

    #[test]
    fn compose_env_array() {
        let yaml = r#"
services:
  app:
    image: myapp
    environment:
      - PORT=8080
      - DEBUG=true
"#;
        let toml = compose_yaml_to_toml(yaml).unwrap();
        assert!(toml.contains("PORT = \"8080\""));
        assert!(toml.contains("DEBUG = \"true\""));
    }

    #[test]
    fn compose_invalid_yaml() {
        assert!(compose_yaml_to_toml("not: [valid: yaml").is_err());
    }

    #[test]
    fn dockerfile_basic() {
        let dockerfile = r#"
FROM alpine:3.19
RUN apk add --no-cache curl
COPY ./app /app
ENV PORT=8080
WORKDIR /app
EXPOSE 8080
ENTRYPOINT ["/app/start.sh"]
USER nobody
"#;
        let toml = dockerfile_to_toml(dockerfile).unwrap();
        assert!(toml.contains("base = \"alpine:3.19\""));
        assert!(toml.contains("type = \"run\""));
        assert!(toml.contains("apk add --no-cache curl"));
        assert!(toml.contains("type = \"copy\""));
        assert!(toml.contains("source = \"./app\""));
        assert!(toml.contains("type = \"env\""));
        assert!(toml.contains("key = \"PORT\""));
        assert!(toml.contains("type = \"workdir\""));
        assert!(toml.contains("expose = [8080]"));
        assert!(toml.contains("entrypoint = [\"/app/start.sh\"]"));
        assert!(toml.contains("user = \"nobody\""));
    }

    #[test]
    fn dockerfile_no_from() {
        assert!(dockerfile_to_toml("RUN echo hello").is_err());
    }

    #[test]
    fn dockerfile_comments_and_blanks() {
        let dockerfile = "# Comment\n\nFROM alpine\n\n# Another comment\nRUN echo hi";
        let toml = dockerfile_to_toml(dockerfile).unwrap();
        assert!(toml.contains("base = \"alpine\""));
        assert!(toml.contains("type = \"run\""));
    }

    #[test]
    fn dockerfile_env_space_format() {
        // ENV KEY VALUE (no equals sign).
        let dockerfile = "FROM alpine\nENV APP_HOME /opt/app";
        let toml = dockerfile_to_toml(dockerfile).unwrap();
        assert!(toml.contains("key = \"APP_HOME\""));
        assert!(toml.contains("value = \"/opt/app\""));
    }

    #[test]
    fn dockerfile_expose_multiple() {
        let dockerfile = "FROM alpine\nEXPOSE 80 443/tcp 8080";
        let toml = dockerfile_to_toml(dockerfile).unwrap();
        assert!(toml.contains("expose = [80, 443, 8080]"));
    }

    #[test]
    fn dockerfile_label() {
        let dockerfile = "FROM alpine\nLABEL maintainer=\"test@example.com\"";
        let toml = dockerfile_to_toml(dockerfile).unwrap();
        assert!(toml.contains("type = \"label\""));
        assert!(toml.contains("key = \"maintainer\""));
    }
}
