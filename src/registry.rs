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
///
/// Also supports OCI artifact manifests (v1.1.0) where `artifact_type`
/// is set and `config.mediaType` is `application/vnd.oci.empty.v1+json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType", default)]
    pub media_type: Option<String>,
    pub config: Descriptor,
    pub layers: Vec<Descriptor>,
    /// OCI artifact type (v1.1.0). Present when this manifest describes
    /// a non-container artifact (e.g., signatures, SBOMs, attestations).
    #[serde(
        rename = "artifactType",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub artifact_type: Option<String>,
    /// Subject descriptor — links this artifact to a parent manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<Descriptor>,
}

/// OCI content descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Descriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: u64,
    /// External URLs for non-distributable (foreign) layers.
    /// When present, the blob should be fetched from these URLs
    /// instead of the registry blob API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<String>>,
    /// Annotations on this descriptor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

impl Descriptor {
    /// Create a new descriptor with required fields only.
    #[must_use]
    pub fn new(media_type: &str, digest: &str, size: u64) -> Self {
        Self {
            media_type: media_type.to_string(),
            digest: digest.to_string(),
            size,
            urls: None,
            annotations: None,
        }
    }

    /// Create a foreign layer descriptor with external URLs.
    #[must_use]
    pub fn foreign(media_type: &str, digest: &str, size: u64, urls: Vec<String>) -> Self {
        Self {
            urls: Some(urls),
            ..Self::new(media_type, digest, size)
        }
    }
}

impl OciManifest {
    /// Create a standard image manifest.
    #[must_use]
    pub fn new(config: Descriptor, layers: Vec<Descriptor>) -> Self {
        Self {
            schema_version: 2,
            media_type: None,
            config,
            layers,
            artifact_type: None,
            subject: None,
        }
    }
}

/// Media type for empty OCI config (used in artifact manifests).
pub(crate) const MEDIA_OCI_EMPTY: &str = "application/vnd.oci.empty.v1+json";

impl OciManifest {
    /// Returns true if this manifest represents an OCI artifact (v1.1.0)
    /// rather than a container image.
    #[must_use]
    pub fn is_artifact(&self) -> bool {
        self.artifact_type.is_some() || self.config.media_type == MEDIA_OCI_EMPTY
    }
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

/// Persistent credential store for registry authentication.
///
/// Stores credentials as JSON at `~/.stiva/credentials.json`.
/// Each entry is keyed by registry hostname.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CredentialStore {
    /// Registry hostname → credentials.
    pub registries: HashMap<String, RegistryCredential>,
}

/// A stored credential for a registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryCredential {
    /// Username.
    pub username: String,
    /// Password or token (stored in plaintext — use OS keyring for production).
    pub password: String,
}

impl CredentialStore {
    /// Default path for the credential store.
    #[must_use]
    pub fn default_path() -> std::path::PathBuf {
        dirs_path().join("credentials.json")
    }

    /// Load credentials from disk.
    pub fn load() -> Result<Self, StivaError> {
        let path = Self::default_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read(&path)
            .map_err(|e| StivaError::Registry(format!("failed to read credentials: {e}")))?;
        serde_json::from_slice(&data)
            .map_err(|e| StivaError::Registry(format!("invalid credentials file: {e}")))
    }

    /// Save credentials to disk.
    pub fn save(&self) -> Result<(), StivaError> {
        let path = Self::default_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(self)?;
        std::fs::write(&path, &data)?;
        Ok(())
    }

    /// Store credentials for a registry.
    pub fn set(&mut self, registry: &str, username: &str, password: &str) {
        self.registries.insert(
            registry.to_string(),
            RegistryCredential {
                username: username.to_string(),
                password: password.to_string(),
            },
        );
    }

    /// Get credentials for a registry.
    #[must_use]
    pub fn get(&self, registry: &str) -> Option<&RegistryCredential> {
        self.registries.get(registry)
    }

    /// Remove credentials for a registry.
    pub fn remove(&mut self, registry: &str) -> bool {
        self.registries.remove(registry).is_some()
    }

    /// Convert to a RegistryConfig for a specific registry.
    #[must_use]
    pub fn to_config(&self, registry: &str) -> RegistryConfig {
        match self.get(registry) {
            Some(cred) => RegistryConfig {
                username: Some(cred.username.clone()),
                password: Some(cred.password.clone()),
            },
            None => RegistryConfig::default(),
        }
    }
}

/// Stiva config directory path (`~/.stiva/`).
fn dirs_path() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".stiva"))
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/.stiva"))
}

/// Fetched manifest — either a single manifest or an index.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ManifestResponse {
    Manifest(Box<OciManifest>),
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

pub(crate) const MEDIA_MANIFEST_V2: &str = "application/vnd.docker.distribution.manifest.v2+json";
pub(crate) const MEDIA_MANIFEST_LIST_V2: &str =
    "application/vnd.docker.distribution.manifest.list.v2+json";
pub(crate) const MEDIA_OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
pub(crate) const MEDIA_OCI_INDEX: &str = "application/vnd.oci.image.index.v1+json";
/// OCI layer media type for zstd-compressed tar archives.
#[allow(dead_code)]
pub(crate) const MEDIA_OCI_LAYER_ZSTD: &str = "application/vnd.oci.image.layer.v1.tar+zstd";

// ---------------------------------------------------------------------------
// RegistryClient
// ---------------------------------------------------------------------------

/// OCI registry client with token caching and multi-arch support.
pub struct RegistryClient {
    client: reqwest::Client,
    config: RegistryConfig,
    /// Cached tokens keyed by `"{registry}\0{scope}"`.
    tokens: Arc<RwLock<HashMap<String, AuthToken>>>,
    /// Override base URL for testing (e.g. `http://localhost:PORT`).
    /// When set, replaces `https://{registry_host}` in all requests.
    #[cfg(test)]
    base_url: Option<String>,
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
            .user_agent(concat!("stiva/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            config,
            tokens: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(test)]
            base_url: None,
        }
    }

    /// Create a client that talks to a mock server instead of real registries.
    #[cfg(test)]
    pub(crate) fn with_base_url(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("stiva/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            config: RegistryConfig::default(),
            tokens: Arc::new(RwLock::new(HashMap::new())),
            base_url: Some(base_url.to_string()),
        }
    }

    // -- internal -----------------------------------------------------------

    /// Get the base URL for API requests.
    fn api_base(&self, image: &ImageRef) -> String {
        #[cfg(test)]
        if let Some(ref base) = self.base_url {
            return base.clone();
        }
        format!("https://{}", registry_host(&image.registry))
    }

    // -- public API ---------------------------------------------------------

    /// Fetch a manifest or manifest list for an image.
    pub async fn fetch_manifest(&self, image: &ImageRef) -> Result<ManifestResponse, StivaError> {
        debug!(image = %image.full_ref(), "fetching manifest");
        let base = self.api_base(image);
        let url = format!(
            "{base}/v2/{}/manifests/{}",
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

        let response = self.authenticated_get(image, &url, Some(&accept)).await?;

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Verify manifest digest if registry provides Docker-Content-Digest header.
        // Defense-in-depth against registry MITM or corruption.
        let expected_digest = response
            .headers()
            .get("docker-content-digest")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let body = response
            .bytes()
            .await
            .map_err(|e| StivaError::Registry(format!("failed to read manifest body: {e}")))?;

        if let Some(ref expected) = expected_digest {
            let actual = crate::image::sha256_digest(&body);
            if actual != *expected {
                return Err(StivaError::Registry(format!(
                    "manifest digest mismatch: expected {expected}, got {actual}"
                )));
            }
            debug!(digest = %actual, "manifest digest verified");
        }

        if content_type.contains("manifest.list") || content_type.contains("image.index") {
            let index: OciIndex = serde_json::from_slice(&body)?;
            Ok(ManifestResponse::Index(index))
        } else {
            let manifest: OciManifest = serde_json::from_slice(&body)?;
            Ok(ManifestResponse::Manifest(Box::new(manifest)))
        }
    }

    /// Resolve a manifest for the current platform.
    ///
    /// If the registry returns a manifest list, selects the entry matching
    /// the current `(os, arch)` and fetches the concrete manifest.
    pub async fn resolve_manifest(&self, image: &ImageRef) -> Result<OciManifest, StivaError> {
        debug!(image = %image.full_ref(), "resolving manifest for current platform");
        match self.fetch_manifest(image).await? {
            ManifestResponse::Manifest(m) => Ok(*m),
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
                    ManifestResponse::Manifest(m) => Ok(*m),
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
        let base = self.api_base(image);
        let url = format!("{base}/v2/{}/blobs/{}", image.repository, digest);

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
        self.authenticated_request(image, reqwest::Method::GET, url, &scope, accept)
            .await
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

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| StivaError::AuthFailed(format!("invalid token response: {e}")))?;

        // Docker Hub uses "token", some registries use "access_token".
        body.get("token")
            .or_else(|| body.get("access_token"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| StivaError::AuthFailed("no token in auth response".into()))
    }

    // -- push API -----------------------------------------------------------

    /// Check if a blob already exists in the registry.
    pub async fn blob_exists(&self, image: &ImageRef, digest: &str) -> Result<bool, StivaError> {
        debug!(digest, "checking if blob exists in registry");
        let base = self.api_base(image);
        let url = format!("{base}/v2/{}/blobs/{digest}", image.repository);
        let scope = format!("repository:{}:pull", image.repository);

        match self
            .authenticated_request(image, reqwest::Method::HEAD, &url, &scope, None)
            .await
        {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Push a blob to the registry (monolithic upload).
    ///
    /// Implements the OCI distribution spec monolithic blob upload:
    /// 1. `POST /v2/{repo}/blobs/uploads/` → get upload URL from `Location` header
    /// 2. `PUT {location}?digest={digest}` with blob data
    pub async fn push_blob(
        &self,
        image: &ImageRef,
        digest: &str,
        data: &[u8],
    ) -> Result<(), StivaError> {
        // Skip if already present.
        if self.blob_exists(image, digest).await? {
            tracing::info!(digest, "blob already exists, skipping upload");
            return Ok(());
        }

        let base = self.api_base(image);
        let scope = format!("repository:{}:push,pull", image.repository);

        // Step 1: Initiate upload.
        let upload_url = format!("{base}/v2/{}/blobs/uploads/", image.repository);
        let resp = self
            .authenticated_request(image, reqwest::Method::POST, &upload_url, &scope, None)
            .await?;

        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| StivaError::Registry("no Location header in upload response".into()))?
            .to_string();

        // Resolve relative location against base URL.
        let put_url = if location.starts_with("http") {
            location
        } else {
            format!("{base}{location}")
        };

        // Step 2: Upload blob in single PUT.
        let sep = if put_url.contains('?') { '&' } else { '?' };
        let final_url = format!("{put_url}{sep}digest={digest}");

        let token = {
            let cache_key = format!("{}\0{}", image.registry, scope);
            self.get_cached_token(&cache_key).await
        };

        let mut req = self
            .client
            .put(&final_url)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .header(reqwest::header::CONTENT_LENGTH, data.len())
            .body(data.to_vec());

        if let Some(ref t) = token {
            req = req.bearer_auth(t);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| StivaError::Registry(format!("blob upload failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(StivaError::Registry(format!(
                "blob upload failed: HTTP {}",
                resp.status()
            )));
        }

        tracing::info!(digest, size = data.len(), "blob pushed");
        Ok(())
    }

    /// Push a blob using chunked upload for large layers.
    ///
    /// Implements the OCI distribution spec chunked upload:
    /// 1. `POST /v2/{repo}/blobs/uploads/` → get upload URL
    /// 2. `PATCH {location}` with chunk data (repeatable)
    /// 3. `PUT {location}?digest={digest}` to finalize
    pub async fn push_blob_chunked(
        &self,
        image: &ImageRef,
        digest: &str,
        data: &[u8],
        chunk_size: usize,
    ) -> Result<(), StivaError> {
        if self.blob_exists(image, digest).await? {
            tracing::info!(digest, "blob already exists, skipping chunked upload");
            return Ok(());
        }

        let base = self.api_base(image);
        let scope = format!("repository:{}:push,pull", image.repository);

        // Step 1: Initiate upload.
        let upload_url = format!("{base}/v2/{}/blobs/uploads/", image.repository);
        let resp = self
            .authenticated_request(image, reqwest::Method::POST, &upload_url, &scope, None)
            .await?;

        let mut location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| StivaError::Registry("no Location header in upload response".into()))?
            .to_string();

        if !location.starts_with("http") {
            location = format!("{base}{location}");
        }

        let cache_key = format!("{}\0{}", image.registry, scope);

        // Step 2: Upload chunks via PATCH.
        let mut offset = 0;
        while offset < data.len() {
            let end = (offset + chunk_size).min(data.len());
            let chunk = &data[offset..end];

            let mut req = self
                .client
                .patch(&location)
                .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                .header(reqwest::header::CONTENT_LENGTH, chunk.len())
                .header("Content-Range", format!("{}-{}", offset, end - 1))
                .body(chunk.to_vec());

            if let Some(ref t) = self.get_cached_token(&cache_key).await {
                req = req.bearer_auth(t);
            }

            let resp = req
                .send()
                .await
                .map_err(|e| StivaError::Registry(format!("chunk upload failed: {e}")))?;

            if let Some(new_loc) = resp.headers().get(reqwest::header::LOCATION)
                && let Ok(s) = new_loc.to_str()
            {
                location = if s.starts_with("http") {
                    s.to_string()
                } else {
                    format!("{base}{s}")
                };
            }

            offset = end;
        }

        // Step 3: Finalize with PUT.
        let sep = if location.contains('?') { '&' } else { '?' };
        let final_url = format!("{location}{sep}digest={digest}");

        let mut req = self
            .client
            .put(&final_url)
            .header(reqwest::header::CONTENT_LENGTH, 0);

        if let Some(ref t) = self.get_cached_token(&cache_key).await {
            req = req.bearer_auth(t);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| StivaError::Registry(format!("chunk finalize failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(StivaError::Registry(format!(
                "chunked upload finalize failed: HTTP {}",
                resp.status()
            )));
        }

        tracing::info!(
            digest,
            size = data.len(),
            chunks = data.len().div_ceil(chunk_size),
            "blob pushed (chunked)"
        );
        Ok(())
    }

    /// Push a manifest to the registry.
    ///
    /// `PUT /v2/{repo}/manifests/{reference}` with the manifest JSON.
    pub async fn push_manifest(
        &self,
        image: &ImageRef,
        manifest: &OciManifest,
    ) -> Result<(), StivaError> {
        let base = self.api_base(image);
        let url = format!("{base}/v2/{}/manifests/{}", image.repository, image.tag);
        let scope = format!("repository:{}:push,pull", image.repository);

        let body = serde_json::to_vec(manifest)?;

        // Ensure we have a push token (probe via authenticated_request).
        let cache_key = format!("{}\0{}", image.registry, scope);
        if self.get_cached_token(&cache_key).await.is_none() {
            // Probe to trigger auth — ignore the response.
            let _ = self
                .authenticated_request(image, reqwest::Method::HEAD, &url, &scope, None)
                .await;
        }

        let mut req = self
            .client
            .put(&url)
            .header(reqwest::header::CONTENT_TYPE, MEDIA_OCI_MANIFEST)
            .body(body);

        if let Some(token) = self.get_cached_token(&cache_key).await {
            req = req.bearer_auth(&token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| StivaError::Registry(format!("manifest push failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(StivaError::Registry(format!(
                "manifest push failed: HTTP {}",
                resp.status()
            )));
        }

        tracing::info!(
            image = %image.full_ref(),
            "manifest pushed"
        );
        Ok(())
    }

    // -- discovery API ------------------------------------------------------

    /// List tags for a repository.
    ///
    /// Implements `GET /v2/{name}/tags/list` with optional pagination.
    pub async fn list_tags(&self, image: &ImageRef) -> Result<Vec<String>, StivaError> {
        let base = self.api_base(image);
        let url = format!("{base}/v2/{}/tags/list", image.repository);

        let response = self.authenticated_get(image, &url, None).await?;
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| StivaError::Registry(format!("failed to parse tag list: {e}")))?;

        let tags = body
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(tags)
    }

    /// List repositories in a registry (catalog).
    ///
    /// Implements `GET /v2/_catalog`. Not all registries support this.
    pub async fn catalog(&self, registry: &str) -> Result<Vec<String>, StivaError> {
        let host = registry_host(registry);
        let url = format!("https://{host}/v2/_catalog");

        // Catalog doesn't need repo-scoped auth, try unauthenticated.
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| StivaError::Registry(format!("catalog request failed: {e}")))?;

        if !response.status().is_success() {
            return Err(StivaError::Registry(format!(
                "catalog failed: HTTP {}",
                response.status()
            )));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| StivaError::Registry(format!("failed to parse catalog: {e}")))?;

        let repos = body
            .get("repositories")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(repos)
    }

    /// Query the referrers API for artifacts referencing a manifest.
    ///
    /// Implements `GET /v2/{name}/referrers/{digest}` (OCI distribution v1.1.0).
    pub async fn referrers(
        &self,
        image: &ImageRef,
        digest: &str,
    ) -> Result<Vec<Descriptor>, StivaError> {
        let base = self.api_base(image);
        let url = format!("{base}/v2/{}/referrers/{digest}", image.repository);

        let response = self.authenticated_get(image, &url, None).await?;
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| StivaError::Registry(format!("failed to parse referrers: {e}")))?;

        let manifests = body
            .get("manifests")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| serde_json::from_value::<Descriptor>(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(manifests)
    }

    // -- generic auth -------------------------------------------------------

    /// Perform an authenticated request with arbitrary method.
    async fn authenticated_request(
        &self,
        image: &ImageRef,
        method: reqwest::Method,
        url: &str,
        scope: &str,
        accept: Option<&str>,
    ) -> Result<reqwest::Response, StivaError> {
        let cache_key = format!("{}\0{}", image.registry, scope);

        // Try with cached token first.
        if let Some(token) = self.get_cached_token(&cache_key).await {
            let mut req = self.client.request(method.clone(), url).bearer_auth(&token);
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

        // Probe for auth challenge.
        let mut req = self.client.request(method.clone(), url);
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
            .ok_or_else(|| StivaError::AuthFailed("401 without WWW-Authenticate header".into()))?
            .to_string();

        let token = self.acquire_token(&www_auth, scope).await?;
        self.cache_token(&cache_key, &token).await;

        // Retry with fresh token.
        let mut req = self.client.request(method, url).bearer_auth(&token);
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
#[must_use]
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
#[must_use = "returns the selected platform manifest"]
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
        assert_eq!(params.get("realm").unwrap(), "https://auth.docker.io/token");
        assert_eq!(params.get("service").unwrap(), "registry.docker.io");
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
    fn registry_client_default() {
        let _client = RegistryClient::default();
    }

    #[test]
    fn registry_client_with_config() {
        let config = RegistryConfig {
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
        };
        let _client = RegistryClient::with_config(config);
    }

    #[test]
    fn parse_www_authenticate_no_prefix() {
        let header = r#"realm="https://auth.example.com/token",service="registry""#;
        let params = parse_www_authenticate(header);
        assert_eq!(
            params.get("realm").unwrap(),
            "https://auth.example.com/token"
        );
    }

    #[test]
    fn normalize_all_arch_values() {
        assert_eq!(normalize_arch("x86_64"), "amd64");
        assert_eq!(normalize_arch("aarch64"), "arm64");
        assert_eq!(normalize_arch("arm"), "arm");
        assert_eq!(normalize_arch("s390x"), "s390x");
        assert_eq!(normalize_arch("powerpc64"), "ppc64le");
        assert_eq!(normalize_arch("riscv64"), "riscv64"); // passthrough
    }

    #[test]
    fn current_platform_is_valid() {
        let p = current_platform();
        assert!(!p.os.is_empty());
        assert!(!p.architecture.is_empty());
    }

    #[test]
    fn platform_selection_with_variant_match() {
        let index = OciIndex {
            schema_version: 2,
            media_type: None,
            manifests: vec![
                PlatformManifest {
                    media_type: MEDIA_MANIFEST_V2.to_string(),
                    digest: "sha256:v7".to_string(),
                    size: 1024,
                    platform: Some(Platform {
                        os: "linux".to_string(),
                        architecture: "arm".to_string(),
                        variant: Some("v7".to_string()),
                        os_version: None,
                    }),
                },
                PlatformManifest {
                    media_type: MEDIA_MANIFEST_V2.to_string(),
                    digest: "sha256:v6".to_string(),
                    size: 1024,
                    platform: Some(Platform {
                        os: "linux".to_string(),
                        architecture: "arm".to_string(),
                        variant: Some("v6".to_string()),
                        os_version: None,
                    }),
                },
            ],
        };

        let target = Platform {
            os: "linux".to_string(),
            architecture: "arm".to_string(),
            variant: Some("v7".to_string()),
            os_version: None,
        };
        let selected = select_platform(&index, &target).unwrap();
        assert_eq!(selected.digest, "sha256:v7");
    }

    #[test]
    fn manifest_response_variants() {
        let manifest = OciManifest::new(
            Descriptor::new(
                "application/vnd.oci.image.config.v1+json",
                "sha256:abc",
                100,
            ),
            vec![],
        );
        let resp = ManifestResponse::Manifest(Box::new(manifest));
        assert!(matches!(resp, ManifestResponse::Manifest(_)));

        let index = OciIndex {
            schema_version: 2,
            media_type: None,
            manifests: vec![],
        };
        let resp = ManifestResponse::Index(index);
        assert!(matches!(resp, ManifestResponse::Index(_)));
    }

    #[tokio::test]
    async fn token_cache_round_trip() {
        let client = RegistryClient::new();
        let key = "test-registry\0repository:lib/nginx:pull";

        // No token cached initially.
        assert!(client.get_cached_token(key).await.is_none());

        // Cache a token.
        client.cache_token(key, "my-secret-token").await;

        // Should be retrievable.
        let token = client.get_cached_token(key).await.unwrap();
        assert_eq!(token, "my-secret-token");
    }

    #[test]
    fn descriptor_serde_round_trip() {
        let desc = Descriptor::new(
            "application/vnd.oci.image.layer.v1.tar+gzip",
            "sha256:abc123",
            1_048_576,
        );
        let json = serde_json::to_string(&desc).unwrap();
        let back: Descriptor = serde_json::from_str(&json).unwrap();
        assert_eq!(back.digest, "sha256:abc123");
        assert_eq!(back.size, 1_048_576);
    }

    #[test]
    fn platform_serde_round_trip() {
        let platform = Platform {
            architecture: "arm64".into(),
            os: "linux".into(),
            variant: Some("v8".into()),
            os_version: Some("22.04".into()),
        };
        let json = serde_json::to_string(&platform).unwrap();
        let back: Platform = serde_json::from_str(&json).unwrap();
        assert_eq!(back, platform);
    }

    #[test]
    fn platform_manifest_serde_round_trip() {
        let pm = PlatformManifest {
            media_type: MEDIA_OCI_MANIFEST.into(),
            digest: "sha256:aabbcc".into(),
            size: 2048,
            platform: Some(Platform {
                architecture: "amd64".into(),
                os: "linux".into(),
                variant: None,
                os_version: None,
            }),
        };
        let json = serde_json::to_string(&pm).unwrap();
        let back: PlatformManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.digest, "sha256:aabbcc");
        assert!(back.platform.is_some());
    }

    #[test]
    fn manifest_serde_round_trip() {
        let manifest = OciManifest {
            media_type: Some(MEDIA_OCI_MANIFEST.into()),
            ..OciManifest::new(
                Descriptor::new(
                    "application/vnd.oci.image.config.v1+json",
                    "sha256:config",
                    512,
                ),
                vec![
                    Descriptor::new(
                        "application/vnd.oci.image.layer.v1.tar+gzip",
                        "sha256:layer1",
                        10_000,
                    ),
                    Descriptor::new(
                        "application/vnd.oci.image.layer.v1.tar+gzip",
                        "sha256:layer2",
                        20_000,
                    ),
                ],
            )
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: OciManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, 2);
        assert_eq!(back.layers.len(), 2);
        assert!(back.media_type.is_some());
    }

    #[test]
    fn index_serde_round_trip() {
        let index = OciIndex {
            schema_version: 2,
            media_type: Some(MEDIA_OCI_INDEX.into()),
            manifests: vec![PlatformManifest {
                media_type: MEDIA_OCI_MANIFEST.into(),
                digest: "sha256:amd64".into(),
                size: 1024,
                platform: Some(Platform {
                    os: "linux".into(),
                    architecture: "amd64".into(),
                    variant: None,
                    os_version: None,
                }),
            }],
        };
        let json = serde_json::to_string(&index).unwrap();
        let back: OciIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(back.manifests.len(), 1);
    }

    #[test]
    fn select_platform_empty_index() {
        let index = OciIndex {
            schema_version: 2,
            media_type: None,
            manifests: vec![],
        };
        let target = Platform {
            os: "linux".into(),
            architecture: "amd64".into(),
            variant: None,
            os_version: None,
        };
        assert!(select_platform(&index, &target).is_err());
    }

    #[test]
    fn select_platform_no_platform_field() {
        let index = OciIndex {
            schema_version: 2,
            media_type: None,
            manifests: vec![PlatformManifest {
                media_type: MEDIA_MANIFEST_V2.into(),
                digest: "sha256:noplat".into(),
                size: 1024,
                platform: None, // No platform specified.
            }],
        };
        let target = Platform {
            os: "linux".into(),
            architecture: "amd64".into(),
            variant: None,
            os_version: None,
        };
        // Should fail — no platform to match against.
        assert!(select_platform(&index, &target).is_err());
    }

    #[test]
    fn select_platform_wrong_os() {
        let index = OciIndex {
            schema_version: 2,
            media_type: None,
            manifests: vec![PlatformManifest {
                media_type: MEDIA_MANIFEST_V2.into(),
                digest: "sha256:win".into(),
                size: 1024,
                platform: Some(Platform {
                    os: "windows".into(),
                    architecture: "amd64".into(),
                    variant: None,
                    os_version: None,
                }),
            }],
        };
        let target = Platform {
            os: "linux".into(),
            architecture: "amd64".into(),
            variant: None,
            os_version: None,
        };
        assert!(select_platform(&index, &target).is_err());
    }

    #[test]
    fn media_type_constants() {
        assert!(MEDIA_MANIFEST_V2.contains("docker"));
        assert!(MEDIA_MANIFEST_LIST_V2.contains("list"));
        assert!(MEDIA_OCI_MANIFEST.contains("oci"));
        assert!(MEDIA_OCI_INDEX.contains("index"));
    }

    #[test]
    fn parse_www_authenticate_empty() {
        let params = parse_www_authenticate("");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_www_authenticate_lowercase_bearer() {
        let header = r#"bearer realm="https://auth.example.com/token""#;
        let params = parse_www_authenticate(header);
        assert_eq!(
            params.get("realm").unwrap(),
            "https://auth.example.com/token"
        );
    }

    #[test]
    fn registry_config_default() {
        let config = RegistryConfig::default();
        assert!(config.username.is_none());
        assert!(config.password.is_none());
    }

    #[test]
    fn platform_equality() {
        let p1 = Platform {
            os: "linux".into(),
            architecture: "amd64".into(),
            variant: None,
            os_version: None,
        };
        let p2 = p1.clone();
        assert_eq!(p1, p2);

        let p3 = Platform {
            os: "linux".into(),
            architecture: "arm64".into(),
            variant: None,
            os_version: None,
        };
        assert_ne!(p1, p3);
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

    // -----------------------------------------------------------------------
    // Wiremock integration tests
    // -----------------------------------------------------------------------

    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Helper: JSON body for a simple OCI manifest.
    fn test_manifest_json() -> String {
        serde_json::to_string(&OciManifest {
            media_type: Some(MEDIA_OCI_MANIFEST.into()),
            ..OciManifest::new(
                Descriptor::new(
                    "application/vnd.oci.image.config.v1+json",
                    "sha256:cfgaaa",
                    64,
                ),
                vec![Descriptor::new(
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                    "sha256:layeraaa",
                    128,
                )],
            )
        })
        .unwrap()
    }

    /// Helper: ImageRef pointing at the mock server.
    fn mock_image(server: &MockServer) -> ImageRef {
        // Use the mock server's address as the registry.
        let addr = server.address().to_string();
        ImageRef {
            registry: addr,
            repository: "library/alpine".into(),
            tag: "latest".into(),
            digest: None,
        }
    }

    // -- fetch_manifest (no auth required) --

    #[tokio::test]
    async fn mock_fetch_manifest_no_auth() {
        let server = MockServer::start().await;
        let body = test_manifest_json();

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(&body)
                    .insert_header("content-type", MEDIA_OCI_MANIFEST),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let resp = client.fetch_manifest(&image).await.unwrap();
        match resp {
            ManifestResponse::Manifest(m) => {
                assert_eq!(m.schema_version, 2);
                assert_eq!(m.layers.len(), 1);
                assert_eq!(m.config.digest, "sha256:cfgaaa");
            }
            ManifestResponse::Index(_) => panic!("expected manifest, got index"),
        }
    }

    // -- fetch_manifest returning manifest list --

    #[tokio::test]
    async fn mock_fetch_manifest_list() {
        let server = MockServer::start().await;
        let arch = normalize_arch(std::env::consts::ARCH);
        let os = std::env::consts::OS;

        let index = OciIndex {
            schema_version: 2,
            media_type: Some(MEDIA_OCI_INDEX.into()),
            manifests: vec![PlatformManifest {
                media_type: MEDIA_OCI_MANIFEST.into(),
                digest: "sha256:platform_digest".into(),
                size: 512,
                platform: Some(Platform {
                    os: os.into(),
                    architecture: arch,
                    variant: None,
                    os_version: None,
                }),
            }],
        };
        let index_json = serde_json::to_string(&index).unwrap();

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", MEDIA_OCI_INDEX)
                    .set_body_raw(index_json, "application/vnd.oci.image.index.v1+json"),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let resp = client.fetch_manifest(&image).await.unwrap();
        assert!(matches!(resp, ManifestResponse::Index(_)));
    }

    // -- fetch_blob --

    #[tokio::test]
    async fn mock_fetch_blob() {
        let server = MockServer::start().await;
        let blob_data = b"fake layer content here";

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/blobs/sha256:layeraaa"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(blob_data.to_vec()))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let data = client.fetch_blob(&image, "sha256:layeraaa").await.unwrap();
        assert_eq!(data.as_ref(), blob_data);
    }

    // -- bearer token auth flow --

    #[tokio::test]
    async fn mock_auth_flow() {
        let server = MockServer::start().await;
        let body = test_manifest_json();

        // First request → 401 with WWW-Authenticate.
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(
                ResponseTemplate::new(401).insert_header(
                    "www-authenticate",
                    &format!(
                        r#"Bearer realm="{}/token",service="registry",scope="repository:library/alpine:pull""#,
                        server.uri()
                    ),
                ),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Token endpoint.
        Mock::given(method("GET"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"token": "test-bearer-token"})),
            )
            .mount(&server)
            .await;

        // Retry with token → success.
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .and(header("authorization", "Bearer test-bearer-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(&body)
                    .insert_header("content-type", MEDIA_OCI_MANIFEST),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let resp = client.fetch_manifest(&image).await.unwrap();
        assert!(matches!(resp, ManifestResponse::Manifest(_)));
    }

    // -- auth flow with access_token key --

    #[tokio::test]
    async fn mock_auth_access_token_key() {
        let server = MockServer::start().await;
        let body = test_manifest_json();

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(401).insert_header(
                "www-authenticate",
                &format!(r#"Bearer realm="{}/token""#, server.uri()),
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Token response uses "access_token" instead of "token".
        Mock::given(method("GET"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"access_token": "alt-token"})),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .and(header("authorization", "Bearer alt-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(&body)
                    .insert_header("content-type", MEDIA_OCI_MANIFEST),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let resp = client.fetch_manifest(&image).await.unwrap();
        assert!(matches!(resp, ManifestResponse::Manifest(_)));
    }

    // -- resolve_manifest (manifest list → concrete manifest) --

    #[tokio::test]
    async fn mock_resolve_manifest_from_index() {
        let server = MockServer::start().await;
        let arch = normalize_arch(std::env::consts::ARCH);
        let os = std::env::consts::OS;

        let index = OciIndex {
            schema_version: 2,
            media_type: Some(MEDIA_OCI_INDEX.into()),
            manifests: vec![PlatformManifest {
                media_type: MEDIA_OCI_MANIFEST.into(),
                digest: "sha256:resolved_digest".into(),
                size: 512,
                platform: Some(Platform {
                    os: os.into(),
                    architecture: arch,
                    variant: None,
                    os_version: None,
                }),
            }],
        };

        // Tag request → index.
        let index_json = serde_json::to_string(&index).unwrap();
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(index_json, MEDIA_OCI_INDEX))
            .mount(&server)
            .await;

        // Digest request → concrete manifest.
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/sha256:resolved_digest"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(test_manifest_json(), MEDIA_OCI_MANIFEST),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let manifest = client.resolve_manifest(&image).await.unwrap();
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.config.digest, "sha256:cfgaaa");
    }

    // -- error: 404 manifest --

    #[tokio::test]
    async fn mock_manifest_not_found() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client.fetch_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::Registry(_)));
    }

    // -- error: 500 server error --

    #[tokio::test]
    async fn mock_server_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client.fetch_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::Registry(_)));
    }

    // -- error: auth failure (401 after token) --

    #[tokio::test]
    async fn mock_auth_failure_after_token() {
        let server = MockServer::start().await;

        // Always 401.
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(401).insert_header(
                "www-authenticate",
                &format!(r#"Bearer realm="{}/token""#, server.uri()),
            ))
            .mount(&server)
            .await;

        // Token endpoint succeeds.
        Mock::given(method("GET"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": "bad-token"})),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client.fetch_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::AuthFailed(_)));
    }

    // -- error: 401 without WWW-Authenticate header --

    #[tokio::test]
    async fn mock_401_no_www_authenticate() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client.fetch_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::AuthFailed(_)));
    }

    // -- error: token endpoint returns no token field --

    #[tokio::test]
    async fn mock_token_endpoint_no_token_field() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(401).insert_header(
                "www-authenticate",
                &format!(r#"Bearer realm="{}/token""#, server.uri()),
            ))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"error": "nope"})),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client.fetch_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::AuthFailed(_)));
    }

    // -- error: token endpoint returns HTTP error --

    #[tokio::test]
    async fn mock_token_endpoint_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(401).insert_header(
                "www-authenticate",
                &format!(r#"Bearer realm="{}/token""#, server.uri()),
            ))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client.fetch_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::AuthFailed(_)));
    }

    // -- blob fetch error --

    #[tokio::test]
    async fn mock_blob_not_found() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/blobs/sha256:missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client
            .fetch_blob(&image, "sha256:missing")
            .await
            .unwrap_err();
        assert!(matches!(err, StivaError::Registry(_)));
    }

    // -- fetch manifest by digest --

    #[tokio::test]
    async fn mock_fetch_manifest_by_digest() {
        let server = MockServer::start().await;
        let body = test_manifest_json();

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/sha256:pinned"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(&body)
                    .insert_header("content-type", MEDIA_OCI_MANIFEST),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let mut image = mock_image(&server);
        image.digest = Some("sha256:pinned".into());
        let resp = client.fetch_manifest(&image).await.unwrap();
        assert!(matches!(resp, ManifestResponse::Manifest(_)));
    }

    // -- cached token reuse --

    #[tokio::test]
    async fn mock_cached_token_reused() {
        let server = MockServer::start().await;
        let body = test_manifest_json();

        // Serve manifest for any request with valid bearer.
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .and(header("authorization", "Bearer cached-tok"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(&body)
                    .insert_header("content-type", MEDIA_OCI_MANIFEST),
            )
            .expect(2) // Should be called twice, both using cached token.
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);

        // Pre-populate cache.
        let cache_key = format!("{}\0repository:{}:pull", image.registry, image.repository);
        client.cache_token(&cache_key, "cached-tok").await;

        // Both calls should use the cached token, no auth dance.
        client.fetch_manifest(&image).await.unwrap();
        client.fetch_manifest(&image).await.unwrap();
    }

    // -- nested manifest list error --

    #[tokio::test]
    async fn mock_nested_manifest_list_error() {
        let server = MockServer::start().await;
        let arch = normalize_arch(std::env::consts::ARCH);
        let os = std::env::consts::OS;

        let index = OciIndex {
            schema_version: 2,
            media_type: Some(MEDIA_OCI_INDEX.into()),
            manifests: vec![PlatformManifest {
                media_type: MEDIA_OCI_MANIFEST.into(),
                digest: "sha256:nested".into(),
                size: 512,
                platform: Some(Platform {
                    os: os.into(),
                    architecture: arch,
                    variant: None,
                    os_version: None,
                }),
            }],
        };
        let index_json = serde_json::to_string(&index).unwrap();

        // Both tag and digest requests return an index (nested).
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(index_json.clone(), MEDIA_OCI_INDEX),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/sha256:nested"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(index_json.clone(), MEDIA_OCI_INDEX),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client.resolve_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::Registry(_)));
        assert!(err.to_string().contains("nested"));
    }

    // -- cached token gets server error (not 401) --

    #[tokio::test]
    async fn mock_cached_token_server_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .and(header("authorization", "Bearer stale-tok"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);

        let cache_key = format!("{}\0repository:{}:pull", image.registry, image.repository);
        client.cache_token(&cache_key, "stale-tok").await;

        let err = client.fetch_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::Registry(_)));
    }

    // -- cached token expired triggers re-auth --

    #[tokio::test]
    async fn mock_expired_token_returns_none() {
        let client = RegistryClient::new();
        let key = "test\0scope";
        {
            let mut tokens = client.tokens.write().await;
            tokens.insert(
                key.to_string(),
                AuthToken {
                    token: "old".into(),
                    expires_at: Some(
                        tokio::time::Instant::now() - std::time::Duration::from_secs(10),
                    ),
                },
            );
        }
        assert!(client.get_cached_token(key).await.is_none());
    }

    #[tokio::test]
    async fn mock_cached_token_401_triggers_reauth() {
        let server = MockServer::start().await;
        let body = test_manifest_json();

        // Success with fresh token — must be registered FIRST so it has lower priority...
        // Actually wiremock uses specificity. Register the specific match first,
        // then the catch-all 401. Wiremock picks the most-specific matching mock.

        // Token endpoint.
        Mock::given(method("GET"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"token": "fresh-token"})),
            )
            .mount(&server)
            .await;

        // 401 for stale token attempt + unauthenticated probe (exactly 2 calls).
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(401).insert_header(
                "www-authenticate",
                &format!(r#"Bearer realm="{}/token""#, server.uri()),
            ))
            .up_to_n_times(2)
            .mount(&server)
            .await;

        // After the 401s are exhausted, this catches the retry with fresh token.
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, MEDIA_OCI_MANIFEST))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);

        // Pre-populate with a stale token the server rejects.
        let cache_key = format!("{}\0repository:{}:pull", image.registry, image.repository);
        client.cache_token(&cache_key, "stale-rejected-tok").await;

        let resp = client.fetch_manifest(&image).await.unwrap();
        assert!(matches!(resp, ManifestResponse::Manifest(_)));
    }

    // -- post-auth-retry gets server error --

    #[tokio::test]
    async fn mock_post_auth_retry_server_error() {
        let server = MockServer::start().await;

        // First: 401 with challenge.
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(401).insert_header(
                "www-authenticate",
                &format!(r#"Bearer realm="{}/token""#, server.uri()),
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": "good-tok"})),
            )
            .mount(&server)
            .await;

        // Retry with token → 500.
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .and(header("authorization", "Bearer good-tok"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client.fetch_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::Registry(_)));
    }

    // -- 401 with WWW-Authenticate missing realm --

    #[tokio::test]
    async fn mock_www_authenticate_no_realm() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(
                ResponseTemplate::new(401)
                    .insert_header("www-authenticate", r#"Bearer service="registry""#),
            )
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image = mock_image(&server);
        let err = client.fetch_manifest(&image).await.unwrap_err();
        assert!(matches!(err, StivaError::AuthFailed(_)));
        assert!(err.to_string().contains("realm"));
    }

    // -- auth with basic credentials --

    #[tokio::test]
    async fn mock_auth_with_basic_credentials() {
        let server = MockServer::start().await;
        let body = test_manifest_json();

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(401).insert_header(
                "www-authenticate",
                &format!(r#"Bearer realm="{}/token",service="reg""#, server.uri()),
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        // Token endpoint expects basic auth header.
        Mock::given(method("GET"))
            .and(path("/token"))
            .and(header("authorization", "Basic dXNlcjpwYXNz")) // base64("user:pass")
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"token": "authed-tok"})),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .and(header("authorization", "Bearer authed-tok"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, MEDIA_OCI_MANIFEST))
            .mount(&server)
            .await;

        let mut client = RegistryClient::with_base_url(&server.uri());
        client.config = RegistryConfig {
            username: Some("user".into()),
            password: Some("pass".into()),
        };

        let image = mock_image(&server);
        let resp = client.fetch_manifest(&image).await.unwrap();
        assert!(matches!(resp, ManifestResponse::Manifest(_)));
    }

    // -- relaxed platform match (variant mismatch, falls to relaxed) --

    #[test]
    fn platform_selection_relaxed_variant_match() {
        let index = OciIndex {
            schema_version: 2,
            media_type: None,
            manifests: vec![PlatformManifest {
                media_type: MEDIA_MANIFEST_V2.into(),
                digest: "sha256:v7only".into(),
                size: 1024,
                platform: Some(Platform {
                    os: "linux".into(),
                    architecture: "arm".into(),
                    variant: Some("v7".into()),
                    os_version: None,
                }),
            }],
        };

        // Target wants v6, only v7 available — should fall to relaxed match.
        let target = Platform {
            os: "linux".into(),
            architecture: "arm".into(),
            variant: Some("v6".into()),
            os_version: None,
        };
        let selected = select_platform(&index, &target).unwrap();
        assert_eq!(selected.digest, "sha256:v7only");
    }

    // -- push tests --

    #[tokio::test]
    async fn mock_blob_exists_true() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .and(path("/v2/library/alpine/blobs/sha256:abc123"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image =
            crate::image::ImageRef::parse(&format!("{}/library/alpine:latest", server.address()))
                .unwrap();

        assert!(client.blob_exists(&image, "sha256:abc123").await.unwrap());
    }

    #[tokio::test]
    async fn mock_blob_exists_false() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .and(path("/v2/library/alpine/blobs/sha256:missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image =
            crate::image::ImageRef::parse(&format!("{}/library/alpine:latest", server.address()))
                .unwrap();

        assert!(!client.blob_exists(&image, "sha256:missing").await.unwrap());
    }

    #[tokio::test]
    async fn mock_push_blob() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // HEAD check: blob does not exist.
        Mock::given(method("HEAD"))
            .and(path("/v2/library/alpine/blobs/sha256:abc"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        // POST initiate upload: return Location.
        Mock::given(method("POST"))
            .and(path("/v2/library/alpine/blobs/uploads/"))
            .respond_with(
                ResponseTemplate::new(202)
                    .append_header("Location", "/v2/library/alpine/blobs/uploads/uuid-123"),
            )
            .mount(&server)
            .await;

        // PUT complete upload.
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image =
            crate::image::ImageRef::parse(&format!("{}/library/alpine:latest", server.address()))
                .unwrap();

        client
            .push_blob(&image, "sha256:abc", b"blob data")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn mock_push_blob_skip_existing() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // HEAD check: blob already exists.
        Mock::given(method("HEAD"))
            .and(path("/v2/library/alpine/blobs/sha256:exists"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image =
            crate::image::ImageRef::parse(&format!("{}/library/alpine:latest", server.address()))
                .unwrap();

        // Should succeed without POST/PUT.
        client
            .push_blob(&image, "sha256:exists", b"data")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn mock_push_manifest() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let client = RegistryClient::with_base_url(&server.uri());
        let image =
            crate::image::ImageRef::parse(&format!("{}/library/alpine:latest", server.address()))
                .unwrap();

        let manifest = OciManifest {
            media_type: Some(MEDIA_OCI_MANIFEST.to_string()),
            ..OciManifest::new(
                Descriptor::new(
                    "application/vnd.oci.image.config.v1+json",
                    "sha256:config",
                    100,
                ),
                vec![Descriptor::new(
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                    "sha256:layer1",
                    4096,
                )],
            )
        };

        client.push_manifest(&image, &manifest).await.unwrap();
    }
}
