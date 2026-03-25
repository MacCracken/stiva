//! Container image building from a TOML-based build specification.
//!
//! Stiva uses `Stivafile.toml` as its build specification format — a structured,
//! typed alternative to Dockerfiles. Each build step is explicit, each field is
//! validated by serde, and the result is an OCI image stored locally.
//!
//! # Example
//!
//! ```toml
//! [image]
//! base = "alpine:3.19"
//! name = "myapp"
//! tag = "latest"
//!
//! [[steps]]
//! type = "run"
//! command = ["apk", "add", "--no-cache", "curl"]
//!
//! [[steps]]
//! type = "copy"
//! source = "./app"
//! destination = "/app"
//!
//! [[steps]]
//! type = "env"
//! key = "PORT"
//! value = "8080"
//!
//! [config]
//! entrypoint = ["/app/start.sh"]
//! expose = [8080]
//! user = "nobody"
//! ```

use crate::error::StivaError;
use crate::image::{Image, ImageRef, ImageStore, Layer};
use crate::registry::RegistryClient;
use flate2::Compression;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A TOML-based image build specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSpec {
    /// Image identity (base image, name, tag).
    pub image: ImageDef,
    /// Ordered build steps.
    #[serde(default)]
    pub steps: Vec<BuildStep>,
    /// Final image configuration (entrypoint, ports, user).
    #[serde(default)]
    pub config: BuildConfig,
}

/// Image identity within a build spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageDef {
    /// Base image reference (e.g. `"alpine:3.19"`).
    pub base: String,
    /// Output image name.
    pub name: String,
    /// Output image tag (defaults to `"latest"`).
    #[serde(default = "default_tag")]
    pub tag: String,
}

/// A single build step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
#[non_exhaustive]
pub enum BuildStep {
    /// Execute a command inside the container.
    Run {
        /// Command and arguments.
        command: Vec<String>,
    },
    /// Copy files from the build context into the image.
    Copy {
        /// Source path relative to the build context.
        source: PathBuf,
        /// Destination path inside the image.
        destination: PathBuf,
    },
    /// Set an environment variable in the image config.
    Env {
        /// Variable name.
        key: String,
        /// Variable value.
        value: String,
    },
    /// Set the working directory in the image config.
    Workdir {
        /// Working directory path inside the container.
        path: PathBuf,
    },
    /// Attach a label to the image.
    Label {
        /// Label key.
        key: String,
        /// Label value.
        value: String,
    },
}

/// Final image configuration applied after all steps.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Container entrypoint command.
    #[serde(default)]
    pub entrypoint: Vec<String>,
    /// Exposed ports.
    #[serde(default)]
    pub expose: Vec<u16>,
    /// User to run as.
    pub user: Option<String>,
    /// Default working directory.
    pub workdir: Option<String>,
}

#[must_use]
fn default_tag() -> String {
    "latest".to_string()
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a TOML string into a [`BuildSpec`].
///
/// # Errors
///
/// Returns [`StivaError::InvalidState`] if the TOML is malformed or missing
/// required fields.
#[must_use = "returns the parsed BuildSpec"]
pub fn parse_build_spec(toml_str: &str) -> Result<BuildSpec, StivaError> {
    info!("parsing build specification");
    let spec: BuildSpec = toml::from_str(toml_str).map_err(|e| {
        let mut msg = String::new();
        let _ = write!(msg, "invalid build spec: {e}");
        StivaError::InvalidState(msg)
    })?;
    debug!(
        base = %spec.image.base,
        name = %spec.image.name,
        tag = %spec.image.tag,
        steps = spec.steps.len(),
        "build spec parsed"
    );
    Ok(spec)
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

/// OCI image config (simplified subset).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OciImageConfig {
    #[serde(default)]
    config: OciContainerConfig,
    #[serde(default)]
    rootfs: OciRootFs,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OciContainerConfig {
    #[serde(rename = "Entrypoint", default, skip_serializing_if = "Vec::is_empty")]
    entrypoint: Vec<String>,
    #[serde(rename = "Env", default)]
    env: Vec<String>,
    #[serde(
        rename = "WorkingDir",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    working_dir: String,
    #[serde(
        rename = "ExposedPorts",
        default,
        skip_serializing_if = "std::collections::HashMap::is_empty"
    )]
    exposed_ports: std::collections::HashMap<String, serde_json::Value>,
    #[serde(rename = "User", default, skip_serializing_if = "String::is_empty")]
    user: String,
    #[serde(
        rename = "Labels",
        default,
        skip_serializing_if = "std::collections::HashMap::is_empty"
    )]
    labels: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct OciRootFs {
    #[serde(rename = "type", default)]
    fs_type: String,
    #[serde(default)]
    diff_ids: Vec<String>,
}

/// Execute a build specification, producing a new [`Image`] stored locally.
///
/// # Process
///
/// 1. Pull the base image from the registry.
/// 2. Walk each build step:
///    - **Run**: create a tar.gz layer with a marker (actual exec requires sandbox).
///    - **Copy**: create a tar.gz layer containing the copied files.
///    - **Env / Workdir / Label**: config-only, no layer produced.
/// 3. Assemble an OCI image config JSON.
/// 4. Store the result in the [`ImageStore`].
///
/// # Errors
///
/// Returns [`StivaError`] on pull failure, I/O errors, or invalid paths.
pub async fn build_image(
    spec: &BuildSpec,
    image_store: &ImageStore,
    registry_client: &RegistryClient,
    context_dir: &Path,
) -> Result<Image, StivaError> {
    info!(
        base = %spec.image.base,
        name = %spec.image.name,
        tag = %spec.image.tag,
        steps = spec.steps.len(),
        "starting image build"
    );

    // 1. Pull base image.
    let base_ref = ImageRef::parse(&spec.image.base)?;
    let base_image = image_store.pull(&base_ref, registry_client).await?;
    info!(base_id = %base_image.id, "base image pulled");

    // 2. Walk steps, accumulating layers and config mutations.
    let mut new_layers: Vec<Layer> = Vec::new();
    let mut env_vars: Vec<String> = Vec::new();
    let mut working_dir = String::new();
    let mut labels: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for (idx, step) in spec.steps.iter().enumerate() {
        match step {
            BuildStep::Run { command } => {
                info!(step = idx, cmd = ?command, "executing run step");
                let layer = build_run_layer(command, idx)?;
                let digest = store_layer(image_store, &layer)?;
                let size = layer.len() as u64;
                new_layers.push(Layer {
                    digest,
                    size_bytes: size,
                    media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
                });
            }
            BuildStep::Copy {
                source,
                destination,
            } => {
                info!(
                    step = idx,
                    src = %source.display(),
                    dst = %destination.display(),
                    "executing copy step"
                );
                let layer = build_copy_layer(context_dir, source, destination)?;
                let digest = store_layer(image_store, &layer)?;
                let size = layer.len() as u64;
                new_layers.push(Layer {
                    digest,
                    size_bytes: size,
                    media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
                });
            }
            BuildStep::Env { key, value } => {
                info!(step = idx, key, value, "recording env var");
                let mut entry = String::new();
                let _ = write!(entry, "{key}={value}");
                env_vars.push(entry);
            }
            BuildStep::Workdir { path } => {
                info!(step = idx, path = %path.display(), "recording workdir");
                working_dir = path.to_string_lossy().into_owned();
            }
            BuildStep::Label { key, value } => {
                info!(step = idx, key, value, "recording label");
                labels.insert(key.clone(), value.clone());
            }
        }
    }

    // 3. Build OCI image config.
    let mut all_layers = base_image.layers.clone();
    all_layers.extend(new_layers.iter().cloned());

    let diff_ids: Vec<String> = all_layers.iter().map(|l| l.digest.clone()).collect();

    let mut exposed_ports = std::collections::HashMap::new();
    for port in &spec.config.expose {
        let mut key = String::new();
        let _ = write!(key, "{port}/tcp");
        exposed_ports.insert(key, serde_json::json!({}));
    }

    let oci_config = OciImageConfig {
        config: OciContainerConfig {
            entrypoint: spec.config.entrypoint.clone(),
            env: env_vars,
            working_dir: spec
                .config
                .workdir
                .clone()
                .unwrap_or_else(|| working_dir.clone()),
            exposed_ports,
            user: spec.config.user.clone().unwrap_or_default(),
            labels,
        },
        rootfs: OciRootFs {
            fs_type: "layers".into(),
            diff_ids,
        },
    };

    let config_bytes = serde_json::to_vec_pretty(&oci_config)?;
    let config_digest = sha256_digest(&config_bytes);
    image_store.store_blob(&config_digest, &config_bytes)?;

    // 4. Build the Image record.
    let total_size: u64 = all_layers.iter().map(|l| l.size_bytes).sum();

    let output_ref = ImageRef {
        registry: "local".into(),
        repository: spec.image.name.clone(),
        tag: spec.image.tag.clone(),
        digest: Some(config_digest.clone()),
    };

    let image = Image {
        id: config_digest,
        reference: output_ref,
        size_bytes: total_size,
        layers: all_layers,
        created_at: chrono::Utc::now(),
    };

    // 5. Persist to image index so it shows up in `stiva images`.
    image_store.add_to_index(&image)?;

    info!(
        id = %image.id,
        name = %spec.image.name,
        tag = %spec.image.tag,
        layers = image.layers.len(),
        size_bytes = total_size,
        "build complete"
    );

    Ok(image)
}

// ---------------------------------------------------------------------------
// Layer helpers
// ---------------------------------------------------------------------------

/// Create a tar.gz layer representing a run step.
///
/// In a full implementation this would execute the command inside a sandbox and
/// capture the filesystem diff. For the initial implementation we create a
/// marker file recording the command.
fn build_run_layer(command: &[String], step_index: usize) -> Result<Vec<u8>, StivaError> {
    let buf = Vec::new();
    let encoder = GzEncoder::new(buf, Compression::fast());
    let mut archive = tar::Builder::new(encoder);

    // Create a marker file: /.stiva/run/<step>.cmd
    let mut marker_path = String::new();
    let _ = write!(marker_path, ".stiva/run/{step_index}.cmd");

    let content = command.join(" ");
    let data = content.as_bytes();

    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();

    archive
        .append_data(&mut header, &marker_path, data)
        .map_err(|e| {
            let mut msg = String::new();
            let _ = write!(msg, "failed to write run layer: {e}");
            StivaError::Storage(msg)
        })?;

    let encoder = archive.into_inner().map_err(|e| {
        let mut msg = String::new();
        let _ = write!(msg, "failed to finish run layer archive: {e}");
        StivaError::Storage(msg)
    })?;

    encoder.finish().map_err(|e| {
        let mut msg = String::new();
        let _ = write!(msg, "failed to compress run layer: {e}");
        StivaError::Storage(msg)
    })
}

/// Create a tar.gz layer copying files from the build context.
fn build_copy_layer(
    context_dir: &Path,
    source: &Path,
    destination: &Path,
) -> Result<Vec<u8>, StivaError> {
    let src_path = context_dir.join(source);
    if !src_path.exists() {
        let mut msg = String::new();
        let _ = write!(msg, "copy source does not exist: {}", src_path.display());
        return Err(StivaError::Storage(msg));
    }

    let buf = Vec::new();
    let encoder = GzEncoder::new(buf, Compression::fast());
    let mut archive = tar::Builder::new(encoder);

    // Strip leading slash from destination for the tar entry path.
    let dest_str = destination
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();

    if src_path.is_dir() {
        debug!(src = %src_path.display(), "adding directory to copy layer");
        append_dir_recursive(&mut archive, &src_path, &dest_str)?;
    } else {
        debug!(src = %src_path.display(), "adding file to copy layer");
        let data = std::fs::read(&src_path)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, &dest_str, data.as_slice())
            .map_err(|e| {
                let mut msg = String::new();
                let _ = write!(msg, "failed to add file to copy layer: {e}");
                StivaError::Storage(msg)
            })?;
    }

    let encoder = archive.into_inner().map_err(|e| {
        let mut msg = String::new();
        let _ = write!(msg, "failed to finish copy layer archive: {e}");
        StivaError::Storage(msg)
    })?;

    encoder.finish().map_err(|e| {
        let mut msg = String::new();
        let _ = write!(msg, "failed to compress copy layer: {e}");
        StivaError::Storage(msg)
    })
}

/// Recursively append a directory to a tar archive.
fn append_dir_recursive<W: std::io::Write>(
    archive: &mut tar::Builder<W>,
    src_dir: &Path,
    dest_prefix: &str,
) -> Result<(), StivaError> {
    let entries = std::fs::read_dir(src_dir)?;
    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        let mut dest_path = String::new();
        if dest_prefix.is_empty() {
            let _ = write!(dest_path, "{file_name_str}");
        } else {
            let _ = write!(dest_path, "{dest_prefix}/{file_name_str}");
        }

        if file_type.is_dir() {
            append_dir_recursive(archive, &entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            let data = std::fs::read(entry.path())?;
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, &dest_path, data.as_slice())
                .map_err(|e| {
                    let mut msg = String::new();
                    let _ = write!(msg, "failed to add {} to layer: {e}", dest_path);
                    StivaError::Storage(msg)
                })?;
        } else {
            warn!(path = %entry.path().display(), "skipping non-regular file in copy");
        }
    }
    Ok(())
}

/// Store a layer blob and return its digest.
fn store_layer(image_store: &ImageStore, data: &[u8]) -> Result<String, StivaError> {
    let digest = sha256_digest(data);
    image_store.store_blob(&digest, data)?;
    debug!(digest = %digest, size = data.len(), "layer stored");
    Ok(digest)
}

/// Compute a `sha256:<hex>` digest.
#[must_use]
fn sha256_digest(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256:");
    for byte in hash {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Parse tests --------------------------------------------------------

    const FULL_SPEC: &str = r#"
[image]
base = "alpine:3.19"
name = "myapp"
tag = "v1.0"

[[steps]]
type = "run"
command = ["apk", "add", "--no-cache", "curl"]

[[steps]]
type = "copy"
source = "./app"
destination = "/app"

[[steps]]
type = "env"
key = "PORT"
value = "8080"

[[steps]]
type = "workdir"
path = "/app"

[[steps]]
type = "label"
key = "maintainer"
value = "stiva"

[[steps]]
type = "run"
command = ["/app/build.sh"]

[config]
entrypoint = ["/app/start.sh"]
expose = [8080]
user = "nobody"
workdir = "/app"
"#;

    #[test]
    fn parse_full_spec() {
        let spec = parse_build_spec(FULL_SPEC).unwrap();
        assert_eq!(spec.image.base, "alpine:3.19");
        assert_eq!(spec.image.name, "myapp");
        assert_eq!(spec.image.tag, "v1.0");
        assert_eq!(spec.steps.len(), 6);
        assert_eq!(spec.config.entrypoint, vec!["/app/start.sh"]);
        assert_eq!(spec.config.expose, vec![8080]);
        assert_eq!(spec.config.user.as_deref(), Some("nobody"));
        assert_eq!(spec.config.workdir.as_deref(), Some("/app"));
    }

    #[test]
    fn parse_minimal_spec() {
        let toml_str = r#"
[image]
base = "ubuntu:22.04"
name = "minimal"

[[steps]]
type = "run"
command = ["echo", "hello"]
"#;
        let spec = parse_build_spec(toml_str).unwrap();
        assert_eq!(spec.image.tag, "latest"); // default
        assert_eq!(spec.steps.len(), 1);
        assert!(spec.config.entrypoint.is_empty());
        assert!(spec.config.expose.is_empty());
        assert!(spec.config.user.is_none());
        assert!(spec.config.workdir.is_none());
    }

    #[test]
    fn parse_no_steps() {
        let toml_str = r#"
[image]
base = "alpine:3.19"
name = "empty"
"#;
        let spec = parse_build_spec(toml_str).unwrap();
        assert!(spec.steps.is_empty());
    }

    #[test]
    fn parse_invalid_toml() {
        let result = parse_build_spec("not valid toml {{{}}}");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid build spec"));
    }

    #[test]
    fn parse_missing_image_section() {
        let result = parse_build_spec("steps = []");
        assert!(result.is_err());
    }

    #[test]
    fn parse_unknown_step_type() {
        let toml_str = r#"
[image]
base = "alpine:3.19"
name = "bad"

[[steps]]
type = "frobnicate"
arg = "baz"
"#;
        let result = parse_build_spec(toml_str);
        assert!(result.is_err());
    }

    // -- Serde round-trip ---------------------------------------------------

    #[test]
    fn serde_round_trip_toml() {
        let spec = parse_build_spec(FULL_SPEC).unwrap();
        let serialized = toml::to_string(&spec).unwrap();
        let deserialized: BuildSpec = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.image.base, spec.image.base);
        assert_eq!(deserialized.image.name, spec.image.name);
        assert_eq!(deserialized.image.tag, spec.image.tag);
        assert_eq!(deserialized.steps.len(), spec.steps.len());
        assert_eq!(deserialized.config.entrypoint, spec.config.entrypoint);
    }

    #[test]
    fn serde_round_trip_json() {
        let spec = parse_build_spec(FULL_SPEC).unwrap();
        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: BuildSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.image.name, spec.image.name);
        assert_eq!(deserialized.steps.len(), spec.steps.len());
    }

    // -- BuildStep variant inspection ---------------------------------------

    #[test]
    fn step_variants() {
        let spec = parse_build_spec(FULL_SPEC).unwrap();

        assert!(matches!(&spec.steps[0], BuildStep::Run { command } if command.len() == 4));
        assert!(
            matches!(&spec.steps[1], BuildStep::Copy { source, destination } if source == Path::new("./app") && destination == Path::new("/app"))
        );
        assert!(
            matches!(&spec.steps[2], BuildStep::Env { key, value } if key == "PORT" && value == "8080")
        );
        assert!(matches!(&spec.steps[3], BuildStep::Workdir { path } if path == Path::new("/app")));
        assert!(
            matches!(&spec.steps[4], BuildStep::Label { key, value } if key == "maintainer" && value == "stiva")
        );
        assert!(matches!(&spec.steps[5], BuildStep::Run { command } if command.len() == 1));
    }

    // -- BuildConfig defaults -----------------------------------------------

    #[test]
    fn build_config_default() {
        let cfg = BuildConfig::default();
        assert!(cfg.entrypoint.is_empty());
        assert!(cfg.expose.is_empty());
        assert!(cfg.user.is_none());
        assert!(cfg.workdir.is_none());
    }

    // -- ImageDef default tag -----------------------------------------------

    #[test]
    fn image_def_default_tag() {
        assert_eq!(default_tag(), "latest");
    }

    // -- Layer building tests -----------------------------------------------

    #[test]
    fn build_run_layer_creates_valid_tar_gz() {
        let data = build_run_layer(&["echo".into(), "hello".into()], 0).unwrap();
        // Should be valid gzip — check magic bytes.
        assert_eq!(data[0], 0x1f);
        assert_eq!(data[1], 0x8b);
        assert!(!data.is_empty());
    }

    #[test]
    fn build_copy_layer_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), b"world").unwrap();

        let data = build_copy_layer(
            dir.path(),
            Path::new("hello.txt"),
            Path::new("/opt/hello.txt"),
        )
        .unwrap();
        assert_eq!(data[0], 0x1f);
        assert_eq!(data[1], 0x8b);
    }

    #[test]
    fn build_copy_layer_directory() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("mydir");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("a.txt"), b"aaa").unwrap();
        std::fs::write(sub.join("b.txt"), b"bbb").unwrap();

        let data =
            build_copy_layer(dir.path(), Path::new("mydir"), Path::new("/opt/mydir")).unwrap();
        assert_eq!(data[0], 0x1f);
        assert_eq!(data[1], 0x8b);
    }

    #[test]
    fn build_copy_layer_missing_source() {
        let dir = tempfile::tempdir().unwrap();
        let result = build_copy_layer(dir.path(), Path::new("nope.txt"), Path::new("/dst"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist"));
    }

    // -- sha256_digest ------------------------------------------------------

    #[test]
    fn sha256_digest_known_value() {
        // SHA-256 of empty data = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let digest = sha256_digest(b"");
        assert_eq!(
            digest,
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_digest_deterministic() {
        let a = sha256_digest(b"hello world");
        let b = sha256_digest(b"hello world");
        assert_eq!(a, b);
    }

    // -- Integration: build_image with mock registry ------------------------

    #[tokio::test]
    async fn build_image_with_mock() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // Set up mock for base image pull.
        let config_data = br#"{"os":"linux"}"#;
        let config_digest = sha256_digest(config_data);

        let manifest = crate::registry::OciManifest {
            schema_version: 2,
            media_type: None,
            config: crate::registry::Descriptor {
                media_type: "application/vnd.oci.image.config.v1+json".into(),
                digest: config_digest.clone(),
                size: config_data.len() as u64,
            },
            layers: vec![],
        };

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/3.19"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                serde_json::to_string(&manifest).unwrap(),
                crate::registry::MEDIA_OCI_MANIFEST,
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/v2/library/alpine/blobs/{config_digest}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(config_data.to_vec()))
            .mount(&server)
            .await;

        // Set up image store and context directory.
        let tmp = tempfile::tempdir().unwrap();
        let image_store = ImageStore::new(&tmp.path().join("images")).unwrap();
        let registry_client = RegistryClient::with_base_url(&server.uri());

        // Create context with a file to copy.
        let ctx_dir = tmp.path().join("context");
        std::fs::create_dir_all(&ctx_dir).unwrap();
        std::fs::write(ctx_dir.join("hello.txt"), b"hello build").unwrap();

        // Use server address as registry in base reference.
        let toml_str = format!(
            r#"
[image]
base = "{}/library/alpine:3.19"
name = "testapp"
tag = "v0.1"

[[steps]]
type = "run"
command = ["echo", "building"]

[[steps]]
type = "copy"
source = "hello.txt"
destination = "/opt/hello.txt"

[[steps]]
type = "env"
key = "APP_ENV"
value = "production"

[[steps]]
type = "workdir"
path = "/opt"

[[steps]]
type = "label"
key = "version"
value = "0.1"

[config]
entrypoint = ["/opt/start"]
expose = [8080, 9090]
user = "app"
workdir = "/opt"
"#,
            server.address()
        );

        let spec = parse_build_spec(&toml_str).unwrap();
        let image = build_image(&spec, &image_store, &registry_client, &ctx_dir)
            .await
            .unwrap();

        // Verify the result.
        assert!(!image.id.is_empty());
        assert!(image.id.starts_with("sha256:"));
        assert_eq!(image.reference.repository, "testapp");
        assert_eq!(image.reference.tag, "v0.1");
        // 0 base layers + 2 new layers (run + copy). Env/Workdir/Label are config-only.
        assert_eq!(image.layers.len(), 2);
        assert!(image.size_bytes > 0);
    }
}
