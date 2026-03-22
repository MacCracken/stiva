//! OCI registry client — pull manifests and blobs from container registries.
//!
//! Implements the [OCI Distribution Specification](https://github.com/opencontainers/distribution-spec)
//! for pulling images from Docker Hub, GHCR, and custom registries.

use crate::error::StivaError;
use crate::image::ImageRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// OCI manifest (v2, schema 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType", default)]
    pub media_type: Option<String>,
    pub config: Descriptor,
    pub layers: Vec<Descriptor>,
}

/// OCI content descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Descriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: u64,
}

/// OCI image index (manifest list) for multi-arch images.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciIndex {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType", default)]
    pub media_type: Option<String>,
    pub manifests: Vec<PlatformManifest>,
}

/// A platform-specific manifest entry within an index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformManifest {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    pub platform: Option<Platform>,
}

/// Platform descriptor (os + architecture + optional variant).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Platform {
    pub architecture: String,
    pub os: String,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(rename = "os.version", default)]
    pub os_version: Option<String>,
}

/// Registry client configuration.
#[derive(Debug, Clone, Default)]
pub struct RegistryConfig {
    /// Optional username for basic auth.
    pub username: Option<String>,
    /// Optional password / personal access token.
    pub password: Option<String>,
}

/// Fetched manifest — either a single manifest or an index.
#[derive(Debug, Clone)]
pub enum ManifestResponse {
    Manifest(OciManifest),
    Index(OciIndex),
}

/// Cached bearer token.
#[derive(Debug, Clone)]
struct AuthToken {
    token: String,
    expires_at: Option<tokio::time::Instant>,
}

// ---------------------------------------------------------------------------
// Media types
// ---------------------------------------------------------------------------

const MEDIA_MANIFEST_V2: &str = "application/vnd.docker.distribution.manifest.v2+json";
const MEDIA_MANIFEST_LIST_V2: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
const MEDIA_OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
const MEDIA_OCI_INDEX: &str = "application/vnd.oci.image.index.v1+json";

// ---------------------------------------------------------------------------
// RegistryClient
// ---------------------------------------------------------------------------

/// OCI registry client with token caching and multi-arch support.
pub struct RegistryClient {
    client: reqwest::Client,
    config: RegistryConfig,
    /// Cached tokens keyed by `"{registry}\0{scope}"`.
    tokens: Arc<RwLock<HashMap<String, AuthToken>>>,
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryClient {
    /// Create a new registry client.
    pub fn new() -> Self {
        Self::with_config(RegistryConfig::default())
    }

    /// Create a registry client with explicit configuration.
    pub fn with_config(config: RegistryConfig) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("stiva/0.21.3")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            config,
            tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // -- public API ---------------------------------------------------------

    /// Fetch a manifest or manifest list for an image.
    pub async fn fetch_manifest(&self, image: &ImageRef) -> Result<ManifestResponse, StivaError> {
        let url = format!(
            "https://{}/v2/{}/manifests/{}",
            registry_host(&image.registry),
            image.repository,
            image.digest.as_deref().unwrap_or(&image.tag),
        );

        let accept = [
            MEDIA_MANIFEST_V2,
            MEDIA_MANIFEST_LIST_V2,
            MEDIA_OCI_MANIFEST,
            MEDIA_OCI_INDEX,
        ]
        .join(", ");

        let response = self
            .authenticated_get(image, &url, Some(&accept))
            .await?;

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = response
            .bytes()
            .await
            .map_err(|e| StivaError::Registry(format!("failed to read manifest body: {e}")))?;

        if content_type.contains("manifest.list") || content_type.contains("image.index") {
            let index: OciIndex = serde_json::from_slice(&body)?;
            Ok(ManifestResponse::Index(index))
        } else {
            let manifest: OciManifest = serde_json::from_slice(&body)?;
            Ok(ManifestResponse::Manifest(manifest))
        }
    }

    /// Resolve a manifest for the current platform.
    ///
    /// If the registry returns a manifest list, selects the entry matching
    /// the current `(os, arch)` and fetches the concrete manifest.
    pub async fn resolve_manifest(&self, image: &ImageRef) -> Result<OciManifest, StivaError> {
        match self.fetch_manifest(image).await? {
            ManifestResponse::Manifest(m) => Ok(m),
            ManifestResponse::Index(index) => {
                let target = current_platform();
                let entry = select_platform(&index, &target)?;
                // Fetch the concrete manifest by digest.
                let pinned = ImageRef {
                    registry: image.registry.clone(),
                    repository: image.repository.clone(),
                    tag: image.tag.clone(),
                    digest: Some(entry.digest.clone()),
                };
                match self.fetch_manifest(&pinned).await? {
                    ManifestResponse::Manifest(m) => Ok(m),
                    ManifestResponse::Index(_) => Err(StivaError::Registry(
                        "nested manifest list not supported".into(),
                    )),
                }
            }
        }
    }

    /// Download a blob (layer or config) as raw bytes.
    pub async fn fetch_blob(
        &self,
        image: &ImageRef,
        digest: &str,
    ) -> Result<bytes::Bytes, StivaError> {
        let url = format!(
            "https://{}/v2/{}/blobs/{}",
            registry_host(&image.registry),
            image.repository,
            digest,
        );

        let response = self.authenticated_get(image, &url, None).await?;

        let status = response.status();
        if !status.is_success() {
            return Err(StivaError::Registry(format!(
                "blob fetch failed: HTTP {status}"
            )));
        }

        response
            .bytes()
            .await
            .map_err(|e| StivaError::Registry(format!("failed to read blob: {e}")))
    }

    // -- auth ---------------------------------------------------------------

    /// Perform a GET with bearer-token auth (auto-acquires on 401).
    async fn authenticated_get(
        &self,
        image: &ImageRef,
        url: &str,
        accept: Option<&str>,
    ) -> Result<reqwest::Response, StivaError> {
        let scope = format!("repository:{}:pull", image.repository);
        let cache_key = format!("{}\0{}", image.registry, scope);

        // Try with cached token first.
        if let Some(token) = self.get_cached_token(&cache_key).await {
            let mut req = self.client.get(url).bearer_auth(&token);
            if let Some(a) = accept {
                req = req.header(reqwest::header::ACCEPT, a);
            }
            let resp = req.send().await?;
            if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
                if resp.status().is_client_error() || resp.status().is_server_error() {
                    return Err(StivaError::Registry(format!(
                        "HTTP {} from {}",
                        resp.status(),
                        url
                    )));
                }
                return Ok(resp);
            }
            debug!("cached token expired, re-authenticating");
        }

        // No cached token or it expired — probe to get auth challenge.
        let mut req = self.client.get(url);
        if let Some(a) = accept {
            req = req.header(reqwest::header::ACCEPT, a);
        }
        let resp = req.send().await?;

        if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
            if resp.status().is_client_error() || resp.status().is_server_error() {
                return Err(StivaError::Registry(format!(
                    "HTTP {} from {}",
                    resp.status(),
                    url
                )));
            }
            return Ok(resp);
        }

        // Parse WWW-Authenticate and acquire token.
        let www_auth = resp
            .headers()
            .get(reqwest::header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                StivaError::AuthFailed("401 without WWW-Authenticate header".into())
            })?
            .to_string();

        let token = self.acquire_token(&www_auth, &scope).await?;
        self.cache_token(&cache_key, &token).await;

        // Retry with fresh token.
        let mut req = self.client.get(url).bearer_auth(&token);
        if let Some(a) = accept {
            req = req.header(reqwest::header::ACCEPT, a);
        }
        let resp = req.send().await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(StivaError::AuthFailed(format!(
                "authentication failed for {}",
                image.full_ref()
            )));
        }
        if resp.status().is_client_error() || resp.status().is_server_error() {
            return Err(StivaError::Registry(format!(
                "HTTP {} from {}",
                resp.status(),
                url
            )));
        }

        Ok(resp)
    }

    /// Parse `WWW-Authenticate: Bearer realm="...",service="...",scope="..."` and fetch a token.
    async fn acquire_token(&self, www_auth: &str, scope: &str) -> Result<String, StivaError> {
        let params = parse_www_authenticate(www_auth);

        let realm = params.get("realm").ok_or_else(|| {
            StivaError::AuthFailed(format!("no realm in WWW-Authenticate: {www_auth}"))
        })?;

        let mut token_url = format!("{realm}?scope={scope}");
        if let Some(service) = params.get("service") {
            token_url.push_str(&format!("&service={service}"));
        }

        let mut req = self.client.get(&token_url);
        if let (Some(u), Some(p)) = (&self.config.username, &self.config.password) {
            req = req.basic_auth(u, Some(p));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| StivaError::AuthFailed(format!("token request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(StivaError::AuthFailed(format!(
                "token endpoint returned HTTP {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| {
            StivaError::AuthFailed(format!("invalid token response: {e}"))
        })?;

        // Docker Hub uses "token", some registries use "access_token".
        body.get("token")
            .or_else(|| body.get("access_token"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| StivaError::AuthFailed("no token in auth response".into()))
    }

    async fn get_cached_token(&self, key: &str) -> Option<String> {
        let tokens = self.tokens.read().await;
        let entry = tokens.get(key)?;
        if let Some(exp) = entry.expires_at
            && tokio::time::Instant::now() >= exp
        {
            return None;
        }
        Some(entry.token.clone())
    }

    async fn cache_token(&self, key: &str, token: &str) {
        let mut tokens = self.tokens.write().await;
        tokens.insert(
            key.to_string(),
            AuthToken {
                token: token.to_string(),
                // Conservative 5-minute expiry (most registries give ~300s).
                expires_at: Some(tokio::time::Instant::now() + std::time::Duration::from_secs(270)),
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map registry shorthand to actual host.
fn registry_host(registry: &str) -> &str {
    match registry {
        "docker.io" => "registry-1.docker.io",
        other => other,
    }
}

/// Parse `Bearer realm="...",service="...",scope="..."` into key-value pairs.
pub(crate) fn parse_www_authenticate(header: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    // Strip "Bearer " prefix.
    let params = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .unwrap_or(header);

    for part in params.split(',') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            let value = value.trim_matches('"');
            map.insert(key.trim().to_string(), value.to_string());
        }
    }
    map
}

/// Detect current platform.
fn current_platform() -> Platform {
    Platform {
        os: std::env::consts::OS.to_string(),
        architecture: normalize_arch(std::env::consts::ARCH),
        variant: None,
        os_version: None,
    }
}

/// Normalize Rust arch strings to OCI arch strings.
fn normalize_arch(arch: &str) -> String {
    match arch {
        "x86_64" => "amd64".to_string(),
        "aarch64" => "arm64".to_string(),
        "arm" => "arm".to_string(),
        "s390x" => "s390x".to_string(),
        "powerpc64" => "ppc64le".to_string(),
        other => other.to_string(),
    }
}

/// Select the best matching platform from an index.
fn select_platform(index: &OciIndex, target: &Platform) -> Result<PlatformManifest, StivaError> {
    // Exact match first.
    for entry in &index.manifests {
        if let Some(p) = &entry.platform
            && p.os == target.os
            && p.architecture == target.architecture
            && (target.variant.is_none() || p.variant == target.variant)
        {
            return Ok(entry.clone());
        }
    }
    // Relaxed match — ignore variant.
    for entry in &index.manifests {
        if let Some(p) = &entry.platform
            && p.os == target.os
            && p.architecture == target.architecture
        {
            warn!(
                "no exact variant match, using {} (variant {:?})",
                entry.digest, p.variant
            );
            return Ok(entry.clone());
        }
    }
    Err(StivaError::UnsupportedPlatform(format!(
        "{}/{}",
        target.os, target.architecture
    )))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_client_creation() {
        let _client = RegistryClient::new();
    }

    #[test]
    fn parse_www_authenticate_bearer() {
        let header = r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/nginx:pull""#;
        let params = parse_www_authenticate(header);
        assert_eq!(
            params.get("realm").unwrap(),
            "https://auth.docker.io/token"
        );
        assert_eq!(
            params.get("service").unwrap(),
            "registry.docker.io"
        );
        assert_eq!(
            params.get("scope").unwrap(),
            "repository:library/nginx:pull"
        );
    }

    #[test]
    fn registry_host_mapping() {
        assert_eq!(registry_host("docker.io"), "registry-1.docker.io");
        assert_eq!(registry_host("ghcr.io"), "ghcr.io");
        assert_eq!(registry_host("quay.io"), "quay.io");
    }

    #[test]
    fn normalize_arch_values() {
        assert_eq!(normalize_arch("x86_64"), "amd64");
        assert_eq!(normalize_arch("aarch64"), "arm64");
    }

    #[test]
    fn platform_selection() {
        let index = OciIndex {
            schema_version: 2,
            media_type: None,
            manifests: vec![
                PlatformManifest {
                    media_type: MEDIA_MANIFEST_V2.to_string(),
                    digest: "sha256:amd64".to_string(),
                    size: 1024,
                    platform: Some(Platform {
                        os: "linux".to_string(),
                        architecture: "amd64".to_string(),
                        variant: None,
                        os_version: None,
                    }),
                },
                PlatformManifest {
                    media_type: MEDIA_MANIFEST_V2.to_string(),
                    digest: "sha256:arm64".to_string(),
                    size: 1024,
                    platform: Some(Platform {
                        os: "linux".to_string(),
                        architecture: "arm64".to_string(),
                        variant: Some("v8".to_string()),
                        os_version: None,
                    }),
                },
            ],
        };

        let target = Platform {
            os: "linux".to_string(),
            architecture: "amd64".to_string(),
            variant: None,
            os_version: None,
        };
        let selected = select_platform(&index, &target).unwrap();
        assert_eq!(selected.digest, "sha256:amd64");

        let target_arm = Platform {
            os: "linux".to_string(),
            architecture: "arm64".to_string(),
            variant: None,
            os_version: None,
        };
        let selected_arm = select_platform(&index, &target_arm).unwrap();
        assert_eq!(selected_arm.digest, "sha256:arm64");
    }

    #[test]
    fn platform_selection_unsupported() {
        let index = OciIndex {
            schema_version: 2,
            media_type: None,
            manifests: vec![PlatformManifest {
                media_type: MEDIA_MANIFEST_V2.to_string(),
                digest: "sha256:abc".to_string(),
                size: 1024,
                platform: Some(Platform {
                    os: "linux".to_string(),
                    architecture: "amd64".to_string(),
                    variant: None,
                    os_version: None,
                }),
            }],
        };

        let target = Platform {
            os: "linux".to_string(),
            architecture: "riscv64".to_string(),
            variant: None,
            os_version: None,
        };
        assert!(select_platform(&index, &target).is_err());
    }

    #[test]
    fn manifest_serde() {
        let json = r#"{
            "schemaVersion": 2,
            "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "digest": "sha256:abc", "size": 1024},
            "layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip", "digest": "sha256:def", "size": 4096}]
        }"#;
        let manifest: OciManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.layers.len(), 1);
    }

    #[test]
    fn index_serde() {
        let json = r#"{
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": "sha256:abc",
                    "size": 1024,
                    "platform": {"architecture": "amd64", "os": "linux"}
                },
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": "sha256:def",
                    "size": 1024,
                    "platform": {"architecture": "arm64", "os": "linux", "variant": "v8"}
                }
            ]
        }"#;
        let index: OciIndex = serde_json::from_str(json).unwrap();
        assert_eq!(index.manifests.len(), 2);
        assert_eq!(
            index.manifests[1].platform.as_ref().unwrap().variant,
            Some("v8".to_string())
        );
    }
}
