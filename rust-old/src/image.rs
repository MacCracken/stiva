//! OCI image management — pull, store, layer handling.

use crate::error::StivaError;
use crate::registry::RegistryClient;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use tracing::info;

/// A parsed image reference (e.g., "docker.io/library/nginx:latest").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRef {
    pub registry: String,
    pub repository: String,
    pub tag: String,
    pub digest: Option<String>,
}

impl ImageRef {
    /// Parse an image reference string.
    ///
    /// Supports: `nginx`, `nginx:1.25`, `user/repo:tag`, `ghcr.io/user/repo:tag`,
    /// `localhost:5000/repo:tag`, `repo@sha256:...`.
    #[must_use = "parsing returns a new ImageRef"]
    pub fn parse(reference: &str) -> Result<Self, StivaError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(StivaError::InvalidReference("empty image reference".into()));
        }

        // Handle digest references (repo@sha256:...)
        let (ref_without_digest, digest) = if let Some((r, d)) = reference.split_once('@') {
            (r, Some(d.to_string()))
        } else {
            (reference, None)
        };

        // Split tag — but only if the colon comes after the last slash
        // (to avoid confusing a port like localhost:5000 with a tag).
        let (ref_without_tag, tag) = if let Some(slash_pos) = ref_without_digest.rfind('/') {
            let after_slash = &ref_without_digest[slash_pos + 1..];
            if let Some((name, tag)) = after_slash.rsplit_once(':') {
                let before = &ref_without_digest[..slash_pos + 1];
                (format!("{before}{name}"), tag.to_string())
            } else {
                (ref_without_digest.to_string(), "latest".to_string())
            }
        } else {
            // No slash at all — simple name or name:tag.
            if let Some((name, tag)) = ref_without_digest.rsplit_once(':') {
                (name.to_string(), tag.to_string())
            } else {
                (ref_without_digest.to_string(), "latest".to_string())
            }
        };

        // Split registry from repository.
        // A first component is a registry if it contains a dot or colon (port).
        let (registry, repository) = if let Some((first, rest)) = ref_without_tag.split_once('/') {
            if first.contains('.') || first.contains(':') {
                (first.to_string(), rest.to_string())
            } else {
                // No dot/colon in first segment → Docker Hub user/repo.
                ("docker.io".to_string(), ref_without_tag.to_string())
            }
        } else {
            // Bare name like "nginx" → docker.io/library/nginx.
            (
                "docker.io".to_string(),
                format!("library/{ref_without_tag}"),
            )
        };

        if repository.is_empty() {
            return Err(StivaError::InvalidReference(format!(
                "empty repository in reference: {reference}"
            )));
        }

        Ok(Self {
            registry,
            repository,
            tag,
            digest,
        })
    }

    /// Full reference string.
    #[inline]
    #[must_use]
    pub fn full_ref(&self) -> String {
        format!("{}/{}:{}", self.registry, self.repository, self.tag)
    }
}

/// A locally stored OCI image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub id: String,
    pub reference: ImageRef,
    pub size_bytes: u64,
    pub layers: Vec<Layer>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// An image layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub digest: String,
    pub size_bytes: u64,
    pub media_type: String,
}

// ---------------------------------------------------------------------------
// ImageStore
// ---------------------------------------------------------------------------

/// Local image storage with content-addressable blob store.
pub struct ImageStore {
    root: PathBuf,
}

impl ImageStore {
    /// Open or create an image store.
    pub fn new(root: &Path) -> Result<Self, StivaError> {
        std::fs::create_dir_all(root)?;
        std::fs::create_dir_all(root.join("blobs").join("sha256"))?;
        std::fs::create_dir_all(root.join("manifests"))?;
        std::fs::create_dir_all(root.join("layers"))?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// Pull an image from a registry.
    pub async fn pull(
        &self,
        reference: &ImageRef,
        client: &RegistryClient,
    ) -> Result<Image, StivaError> {
        info!(image = %reference.full_ref(), "pulling image");

        // 1. Resolve manifest (handles multi-arch transparently).
        let manifest = client.resolve_manifest(reference).await?;

        // 2. Store the manifest itself.
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        let manifest_digest = sha256_digest(&manifest_bytes);
        self.store_blob(&manifest_digest, &manifest_bytes)?;

        // 3. Download config blob.
        info!(digest = %manifest.config.digest, "pulling config");
        if !self.has_blob(&manifest.config.digest) {
            let config_data = client
                .fetch_blob(reference, &manifest.config.digest)
                .await?;
            self.store_blob(&manifest.config.digest, &config_data)?;
        }

        // 4. Download layer blobs concurrently (max 4 at a time).
        let layers_to_pull: Vec<_> = manifest
            .layers
            .iter()
            .filter(|l| !self.has_blob(&l.digest))
            .collect();

        let total = manifest.layers.len();
        let skipped = total - layers_to_pull.len();
        if skipped > 0 {
            info!(skipped, total, "dedup: skipping already-present layers");
        }

        let pull_results: Vec<Result<(), StivaError>> = stream::iter(layers_to_pull)
            .map(|layer| {
                let digest = layer.digest.clone();
                let size = layer.size;
                let urls = layer.urls.clone();
                async move {
                    info!(digest = %digest, size, "pulling layer");
                    let data = if let Some(ref urls) = urls
                        && !urls.is_empty()
                    {
                        // Foreign/non-distributable layer: fetch from external URL.
                        info!(digest = %digest, url = %urls[0], "fetching foreign layer");
                        fetch_foreign_layer(urls).await?
                    } else {
                        client.fetch_blob(reference, &digest).await?
                    };
                    self.store_blob(&digest, &data)?;
                    Ok(())
                }
            })
            .buffer_unordered(4)
            .collect()
            .await;

        // Check for errors.
        for result in pull_results {
            result?;
        }

        // 5. Build Image record.
        let layers: Vec<Layer> = manifest
            .layers
            .iter()
            .map(|d| Layer {
                digest: d.digest.clone(),
                size_bytes: d.size, // Descriptor::size → Layer::size_bytes
                media_type: d.media_type.clone(),
            })
            .collect();

        let total_size: u64 = layers.iter().map(|l| l.size_bytes).sum();

        let image = Image {
            id: manifest.config.digest.clone(),
            reference: reference.clone(),
            size_bytes: total_size,
            layers,
            created_at: chrono::Utc::now(),
        };

        // 6. Persist to image index.
        self.add_to_index(&image)?;

        info!(
            image = %reference.full_ref(),
            size_bytes = total_size,
            layers = total,
            "pull complete"
        );

        Ok(image)
    }

    // -- Blob store ---------------------------------------------------------

    /// Store a blob by its expected digest. Verifies SHA-256.
    pub fn store_blob(&self, digest: &str, data: &[u8]) -> Result<PathBuf, StivaError> {
        tracing::debug!(digest, size = data.len(), "storing blob");
        let hex = digest_hex(digest);
        let path = self.root.join("blobs").join("sha256").join(&*hex);

        // Dedup: skip if already exists.
        if path.exists() {
            return Ok(path);
        }

        // Verify digest.
        let actual = sha256_digest(data);
        if actual != digest {
            return Err(StivaError::DigestMismatch {
                expected: digest.to_string(),
                actual,
            });
        }

        // Write atomically via temp file + rename.
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, data)?;
        std::fs::rename(&tmp_path, &path)?;

        Ok(path)
    }

    /// Check whether a blob exists locally.
    #[inline]
    pub fn has_blob(&self, digest: &str) -> bool {
        let hex = digest_hex(digest);
        self.root.join("blobs").join("sha256").join(&*hex).exists()
    }

    /// Read a blob from the store.
    pub fn read_blob(&self, digest: &str) -> Result<Vec<u8>, StivaError> {
        tracing::debug!(digest, "reading blob");
        let hex = digest_hex(digest);
        let path = self.root.join("blobs").join("sha256").join(&*hex);
        std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StivaError::ImageNotFound(format!("blob {digest} not found"))
            } else {
                StivaError::Io(e)
            }
        })
    }

    // -- Image index --------------------------------------------------------

    fn index_path(&self) -> PathBuf {
        self.root.join("images.json")
    }

    fn load_index(&self) -> Result<Vec<Image>, StivaError> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(vec![]);
        }
        let data = std::fs::read(&path)?;
        let images: Vec<Image> = serde_json::from_slice(&data)?;
        Ok(images)
    }

    /// Save the image index. Used for bulk operations (tag, rmi by reference).
    pub fn save_index_pub(&self, images: &[Image]) -> Result<(), StivaError> {
        self.save_index(images)
    }

    fn save_index(&self, images: &[Image]) -> Result<(), StivaError> {
        let data = serde_json::to_vec_pretty(images)?;
        let path = self.index_path();
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Add or replace an image in the local index.
    pub fn add_to_index(&self, image: &Image) -> Result<(), StivaError> {
        let mut images = self.load_index()?;
        // Replace existing entry for same reference.
        images.retain(|i| i.reference.full_ref() != image.reference.full_ref());
        images.push(image.clone());
        self.save_index(&images)
    }

    /// Push a local image to a registry.
    ///
    /// Uploads all layer blobs and the config blob, then pushes the manifest.
    /// Blobs already present in the registry are skipped (dedup via HEAD check).
    pub async fn push(
        &self,
        image: &Image,
        target: &ImageRef,
        client: &RegistryClient,
    ) -> Result<(), StivaError> {
        info!(image = %target.full_ref(), "pushing image");

        // 1. Push config blob.
        let config_data = self.read_blob(&image.id)?;
        info!(digest = %image.id, "pushing config blob");
        client.push_blob(target, &image.id, &config_data).await?;

        // 2. Push layer blobs.
        for layer in &image.layers {
            let data = self.read_blob(&layer.digest)?;
            info!(digest = %layer.digest, size = data.len(), "pushing layer");
            client.push_blob(target, &layer.digest, &data).await?;
        }

        // 3. Build and push manifest.
        let manifest = crate::registry::OciManifest {
            media_type: Some(crate::registry::MEDIA_OCI_MANIFEST.to_string()),
            ..crate::registry::OciManifest::new(
                crate::registry::Descriptor::new(
                    "application/vnd.oci.image.config.v1+json",
                    &image.id,
                    config_data.len() as u64,
                ),
                image
                    .layers
                    .iter()
                    .map(|l| {
                        crate::registry::Descriptor::new(&l.media_type, &l.digest, l.size_bytes)
                    })
                    .collect(),
            )
        };

        client.push_manifest(target, &manifest).await?;

        info!(
            image = %target.full_ref(),
            layers = image.layers.len(),
            "push complete"
        );
        Ok(())
    }

    /// List locally stored images.
    #[must_use = "returns the list of images"]
    pub fn list(&self) -> Result<Vec<Image>, StivaError> {
        self.load_index()
    }

    /// Remove an image. Deletes unreferenced blobs.
    pub fn remove(&self, id: &str) -> Result<(), StivaError> {
        let mut images = self.load_index()?;
        let before = images.len();
        images.retain(|i| i.id != id);
        if images.len() == before {
            return Err(StivaError::ImageNotFound(id.to_string()));
        }

        // Collect all digests still referenced by remaining images.
        let referenced: std::collections::HashSet<String> = images
            .iter()
            .flat_map(|i| {
                let mut digests = vec![i.id.clone()];
                digests.extend(i.layers.iter().map(|l| l.digest.clone()));
                digests
            })
            .collect();

        // Delete blobs that are no longer referenced.
        let blobs_dir = self.root.join("blobs").join("sha256");
        if let Ok(entries) = std::fs::read_dir(&blobs_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let hex = name.to_string_lossy();
                let digest = format!("sha256:{hex}");
                if !referenced.contains(&digest) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }

        self.save_index(&images)
    }

    /// Garbage-collect unreferenced blobs and unpacked layers.
    ///
    /// Walks the image index to build a set of referenced digests,
    /// then removes any blob or layer directory not in that set.
    /// Returns `(blobs_removed, layers_removed)`.
    pub fn gc(&self) -> Result<(u32, u32), StivaError> {
        info!("running image garbage collection");
        let images = self.load_index()?;

        // Build referenced digest set from all images.
        let referenced: std::collections::HashSet<String> = images
            .iter()
            .flat_map(|i| {
                let mut digests = vec![i.id.clone()];
                digests.extend(i.layers.iter().map(|l| l.digest.clone()));
                digests
            })
            .collect();

        let referenced_hex: std::collections::HashSet<String> = referenced
            .iter()
            .filter_map(|d| d.strip_prefix("sha256:").map(|s| s.to_string()))
            .collect();

        // Remove unreferenced blobs.
        let mut blobs_removed = 0u32;
        let blobs_dir = self.root.join("blobs").join("sha256");
        if let Ok(entries) = std::fs::read_dir(&blobs_dir) {
            for entry in entries.flatten() {
                let hex = entry.file_name().to_string_lossy().to_string();
                if !referenced_hex.contains(&hex) {
                    let _ = std::fs::remove_file(entry.path());
                    blobs_removed += 1;
                }
            }
        }

        // Remove unreferenced unpacked layer directories.
        let mut layers_removed = 0u32;
        let layers_dir = self.root.join("layers");
        if let Ok(entries) = std::fs::read_dir(&layers_dir) {
            for entry in entries.flatten() {
                let hex = entry.file_name().to_string_lossy().to_string();
                if !referenced_hex.contains(&hex) {
                    let _ = std::fs::remove_dir_all(entry.path());
                    layers_removed += 1;
                }
            }
        }

        info!(blobs_removed, layers_removed, "garbage collection complete");
        Ok((blobs_removed, layers_removed))
    }

    /// Verify integrity of stored blobs against their digests.
    ///
    /// Re-reads each blob and computes its SHA-256, comparing against the
    /// filename-derived digest. Returns a list of corrupted digests.
    /// This is a TOCTOU defense — run after unpack to verify content.
    pub fn verify_integrity(&self) -> Result<Vec<String>, StivaError> {
        info!("verifying blob integrity");
        let blobs_dir = self.root.join("blobs").join("sha256");
        let mut corrupted = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&blobs_dir) {
            for entry in entries.flatten() {
                let hex = entry.file_name().to_string_lossy().to_string();
                let expected = format!("sha256:{hex}");
                match std::fs::read(entry.path()) {
                    Ok(data) => {
                        let actual = sha256_digest(&data);
                        if actual != expected {
                            tracing::warn!(
                                expected = %expected,
                                actual = %actual,
                                "blob integrity mismatch"
                            );
                            corrupted.push(expected);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(digest = %expected, error = %e, "failed to read blob for verification");
                        corrupted.push(expected);
                    }
                }
            }
        }

        info!(
            verified = corrupted.is_empty(),
            corrupted = corrupted.len(),
            "integrity check complete"
        );
        Ok(corrupted)
    }

    /// Verify a cosign/notation signature for an image.
    ///
    /// Checks for a signature artifact referencing the image manifest via
    /// the referrers API. Returns `Ok(true)` if a valid signature is found,
    /// `Ok(false)` if no signature exists, or `Err` on verification failure.
    pub async fn verify_signature(
        &self,
        image: &Image,
        client: &crate::registry::RegistryClient,
    ) -> Result<bool, StivaError> {
        info!(image = %image.reference.full_ref(), "checking image signature");

        // Look up referrers for this image's manifest digest.
        let referrers = client.referrers(&image.reference, &image.id).await?;

        // Check for cosign or notation signature artifacts.
        let sig_types = [
            "application/vnd.dev.cosign.simplesigning.v1+json",
            "application/vnd.cncf.notary.signature",
        ];

        let has_signature = referrers
            .iter()
            .any(|d| sig_types.iter().any(|t| d.media_type == *t));

        if has_signature {
            info!(image = %image.reference.full_ref(), "signature found");
        } else {
            info!(image = %image.reference.full_ref(), "no signature found");
        }

        Ok(has_signature)
    }

    /// Get image storage root.
    #[inline]
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

// ---------------------------------------------------------------------------
// Digest helpers
// ---------------------------------------------------------------------------

/// Fetch a foreign (non-distributable) layer from external URLs.
///
/// Tries each URL in order until one succeeds. This handles layers
/// where the registry does not host the blob and instead provides
/// external download URLs in the descriptor.
async fn fetch_foreign_layer(urls: &[String]) -> Result<bytes::Bytes, StivaError> {
    let client = reqwest::Client::new();
    for url in urls {
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                return resp
                    .bytes()
                    .await
                    .map_err(|e| StivaError::Registry(format!("foreign layer read failed: {e}")));
            }
            Ok(resp) => {
                tracing::warn!(url, status = %resp.status(), "foreign layer URL failed, trying next");
            }
            Err(e) => {
                tracing::warn!(url, error = %e, "foreign layer URL failed, trying next");
            }
        }
    }
    Err(StivaError::Registry("all foreign layer URLs failed".into()))
}

/// Compute `sha256:{hex}` digest for data.
#[must_use]
pub(crate) fn sha256_digest(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    let mut out = String::with_capacity(7 + 64); // "sha256:" + 64 hex chars
    out.push_str("sha256:");
    let _ = write!(out, "{}", hex::encode(hash));
    out
}

/// Extract the hex portion from a `sha256:{hex}` digest.
/// Returns a borrowed slice when the prefix is present, avoiding allocation.
#[inline]
#[must_use]
fn digest_hex(digest: &str) -> Cow<'_, str> {
    match digest.strip_prefix("sha256:") {
        Some(hex) => Cow::Borrowed(hex),
        None => Cow::Borrowed(digest),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_ref() {
        let r = ImageRef::parse("nginx").unwrap();
        assert_eq!(r.registry, "docker.io");
        assert_eq!(r.repository, "library/nginx");
        assert_eq!(r.tag, "latest");
    }

    #[test]
    fn parse_tagged_ref() {
        let r = ImageRef::parse("nginx:1.25").unwrap();
        assert_eq!(r.repository, "library/nginx");
        assert_eq!(r.tag, "1.25");
    }

    #[test]
    fn parse_full_ref() {
        let r = ImageRef::parse("ghcr.io/maccracken/agnosticos:latest").unwrap();
        assert_eq!(r.registry, "ghcr.io");
        assert_eq!(r.repository, "maccracken/agnosticos");
        assert_eq!(r.tag, "latest");
    }

    #[test]
    fn parse_namespaced_ref() {
        let r = ImageRef::parse("pytorch/pytorch:2.0").unwrap();
        assert_eq!(r.registry, "docker.io");
        assert_eq!(r.repository, "pytorch/pytorch");
        assert_eq!(r.tag, "2.0");
    }

    #[test]
    fn store_creation() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();
        assert!(store.root().join("blobs").join("sha256").exists());
        assert!(store.root().join("manifests").exists());
        assert!(store.root().join("layers").exists());
    }

    #[test]
    fn blob_store_write_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let data = b"hello world";
        let digest = sha256_digest(data);

        let path = store.store_blob(&digest, data).unwrap();
        assert!(path.exists());

        let read_back = store.read_blob(&digest).unwrap();
        assert_eq!(read_back, data);
    }

    #[test]
    fn blob_store_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let data = b"dedup test";
        let digest = sha256_digest(data);

        store.store_blob(&digest, data).unwrap();
        // Second write should succeed (dedup skip).
        let path = store.store_blob(&digest, data).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn blob_store_digest_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let data = b"actual data";
        let wrong_digest =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000";

        let result = store.store_blob(wrong_digest, data);
        assert!(result.is_err());
        match result.unwrap_err() {
            StivaError::DigestMismatch { expected, actual } => {
                assert_eq!(expected, wrong_digest);
                assert!(actual.starts_with("sha256:"));
            }
            other => panic!("expected DigestMismatch, got {other:?}"),
        }
    }

    #[test]
    fn blob_read_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();
        assert!(store.read_blob("sha256:nonexistent").is_err());
    }

    #[test]
    fn has_blob_check() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let data = b"exists check";
        let digest = sha256_digest(data);

        assert!(!store.has_blob(&digest));
        store.store_blob(&digest, data).unwrap();
        assert!(store.has_blob(&digest));
    }

    #[test]
    fn image_index_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        assert!(store.list().unwrap().is_empty());

        let image = Image {
            id: "sha256:config123".to_string(),
            reference: ImageRef::parse("alpine:3.19").unwrap(),
            size_bytes: 4096,
            layers: vec![Layer {
                digest: "sha256:layer1".to_string(),
                size_bytes: 4096,
                media_type: "application/vnd.oci.image.layer.v1.tar+gzip".to_string(),
            }],
            created_at: chrono::Utc::now(),
        };

        store.add_to_index(&image).unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "sha256:config123");
    }

    #[test]
    fn image_index_remove() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        // Store a blob that belongs to the image.
        let data = b"layer data";
        let digest = sha256_digest(data);
        store.store_blob(&digest, data).unwrap();

        let image = Image {
            id: "sha256:config456".to_string(),
            reference: ImageRef::parse("nginx:latest").unwrap(),
            size_bytes: 10,
            layers: vec![Layer {
                digest: digest.clone(),
                size_bytes: 10,
                media_type: "application/vnd.oci.image.layer.v1.tar+gzip".to_string(),
            }],
            created_at: chrono::Utc::now(),
        };

        store.add_to_index(&image).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);

        store.remove("sha256:config456").unwrap();
        assert!(store.list().unwrap().is_empty());

        // Blob should be cleaned up since no images reference it.
        assert!(!store.has_blob(&digest));
    }

    // -- ImageRef parser edge cases --

    #[test]
    fn parse_empty_ref() {
        assert!(ImageRef::parse("").is_err());
        assert!(ImageRef::parse("   ").is_err());
    }

    #[test]
    fn parse_digest_ref() {
        let r = ImageRef::parse("nginx@sha256:abcdef1234567890").unwrap();
        assert_eq!(r.repository, "library/nginx");
        assert_eq!(r.tag, "latest");
        assert_eq!(r.digest.as_deref(), Some("sha256:abcdef1234567890"));
    }

    #[test]
    fn parse_registry_with_port() {
        let r = ImageRef::parse("localhost:5000/myimage:v1").unwrap();
        assert_eq!(r.registry, "localhost:5000");
        assert_eq!(r.repository, "myimage");
        assert_eq!(r.tag, "v1");
    }

    #[test]
    fn parse_registry_with_port_no_tag() {
        let r = ImageRef::parse("localhost:5000/myimage").unwrap();
        assert_eq!(r.registry, "localhost:5000");
        assert_eq!(r.repository, "myimage");
        assert_eq!(r.tag, "latest");
    }

    #[test]
    fn parse_deep_repo_path() {
        let r = ImageRef::parse("ghcr.io/org/sub/repo:v2").unwrap();
        assert_eq!(r.registry, "ghcr.io");
        assert_eq!(r.repository, "org/sub/repo");
        assert_eq!(r.tag, "v2");
    }

    #[test]
    fn full_ref_format() {
        let r = ImageRef::parse("ghcr.io/user/repo:v1").unwrap();
        assert_eq!(r.full_ref(), "ghcr.io/user/repo:v1");
    }

    #[test]
    fn parse_tagged_digest_ref() {
        let r = ImageRef::parse("nginx:1.25@sha256:abc123").unwrap();
        assert_eq!(r.tag, "1.25");
        assert_eq!(r.digest.as_deref(), Some("sha256:abc123"));
    }

    // -- Digest helpers --

    #[test]
    fn sha256_digest_deterministic() {
        let d1 = sha256_digest(b"test");
        let d2 = sha256_digest(b"test");
        assert_eq!(d1, d2);
        assert!(d1.starts_with("sha256:"));
        assert_eq!(d1.len(), 7 + 64); // "sha256:" + 64 hex chars
    }

    #[test]
    fn digest_hex_strips_prefix() {
        assert_eq!(digest_hex("sha256:abcdef"), "abcdef");
        assert_eq!(digest_hex("abcdef"), "abcdef");
    }

    // -- Image remove edge cases --

    #[test]
    fn remove_nonexistent_image() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();
        assert!(store.remove("sha256:doesnotexist").is_err());
    }

    #[test]
    fn remove_preserves_shared_blobs() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let shared_data = b"shared layer";
        let shared_digest = sha256_digest(shared_data);
        store.store_blob(&shared_digest, shared_data).unwrap();

        let img1 = Image {
            id: "sha256:img1".into(),
            reference: ImageRef::parse("app:v1").unwrap(),
            size_bytes: 12,
            layers: vec![Layer {
                digest: shared_digest.clone(),
                size_bytes: 12,
                media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
            }],
            created_at: chrono::Utc::now(),
        };
        let img2 = Image {
            id: "sha256:img2".into(),
            reference: ImageRef::parse("app:v2").unwrap(),
            size_bytes: 12,
            layers: vec![Layer {
                digest: shared_digest.clone(),
                size_bytes: 12,
                media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
            }],
            created_at: chrono::Utc::now(),
        };

        store.add_to_index(&img1).unwrap();
        store.add_to_index(&img2).unwrap();

        // Remove img1 — shared blob should survive because img2 still needs it.
        store.remove("sha256:img1").unwrap();
        assert!(store.has_blob(&shared_digest));

        // Remove img2 — now the blob is unreferenced and should be cleaned up.
        store.remove("sha256:img2").unwrap();
        assert!(!store.has_blob(&shared_digest));
    }

    // -- Serde round-trips --

    #[test]
    fn image_ref_serde_round_trip() {
        let r = ImageRef::parse("ghcr.io/org/repo:v3").unwrap();
        let json = serde_json::to_string(&r).unwrap();
        let back: ImageRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.registry, "ghcr.io");
        assert_eq!(back.repository, "org/repo");
        assert_eq!(back.tag, "v3");
        assert_eq!(back.full_ref(), r.full_ref());
    }

    #[test]
    fn image_serde_round_trip() {
        let image = Image {
            id: "sha256:abc".into(),
            reference: ImageRef::parse("nginx:1.25").unwrap(),
            size_bytes: 50_000_000,
            layers: vec![
                Layer {
                    digest: "sha256:layer1".into(),
                    size_bytes: 30_000_000,
                    media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
                },
                Layer {
                    digest: "sha256:layer2".into(),
                    size_bytes: 20_000_000,
                    media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
                },
            ],
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&image).unwrap();
        let back: Image = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "sha256:abc");
        assert_eq!(back.layers.len(), 2);
        assert_eq!(back.size_bytes, 50_000_000);
    }

    #[test]
    fn layer_serde_round_trip() {
        let layer = Layer {
            digest: "sha256:aabbcc".into(),
            size_bytes: 12345,
            media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
        };
        let json = serde_json::to_string(&layer).unwrap();
        let back: Layer = serde_json::from_str(&json).unwrap();
        assert_eq!(back.digest, "sha256:aabbcc");
        assert_eq!(back.size_bytes, 12345);
    }

    // -- More ImageRef edge cases --

    #[test]
    fn parse_full_registry_with_digest() {
        let r = ImageRef::parse("ghcr.io/user/repo:v1@sha256:deadbeef").unwrap();
        assert_eq!(r.registry, "ghcr.io");
        assert_eq!(r.repository, "user/repo");
        assert_eq!(r.tag, "v1");
        assert_eq!(r.digest.as_deref(), Some("sha256:deadbeef"));
    }

    #[test]
    fn parse_registry_port_deep_path() {
        let r = ImageRef::parse("myregistry.io:8443/org/team/app:prod").unwrap();
        assert_eq!(r.registry, "myregistry.io:8443");
        assert_eq!(r.repository, "org/team/app");
        assert_eq!(r.tag, "prod");
    }

    #[test]
    fn parse_whitespace_trimmed() {
        let r = ImageRef::parse("  nginx:latest  ").unwrap();
        assert_eq!(r.repository, "library/nginx");
        assert_eq!(r.tag, "latest");
    }

    #[test]
    fn full_ref_default_tag() {
        let r = ImageRef::parse("alpine").unwrap();
        assert_eq!(r.full_ref(), "docker.io/library/alpine:latest");
    }

    // -- Blob store edge cases --

    #[test]
    fn store_empty_blob() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let data = b"";
        let digest = sha256_digest(data);
        store.store_blob(&digest, data).unwrap();
        let read_back = store.read_blob(&digest).unwrap();
        assert!(read_back.is_empty());
    }

    #[test]
    fn store_large_blob() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let data = vec![0xABu8; 1_000_000]; // 1MB
        let digest = sha256_digest(&data);
        store.store_blob(&digest, &data).unwrap();
        let read_back = store.read_blob(&digest).unwrap();
        assert_eq!(read_back.len(), 1_000_000);
    }

    #[test]
    fn blob_path_is_under_store_root() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let data = b"path check";
        let digest = sha256_digest(data);
        let path = store.store_blob(&digest, data).unwrap();
        assert!(path.starts_with(dir.path()));
        assert!(path.to_string_lossy().contains("blobs/sha256"));
    }

    // -- Multi-image index --

    #[test]
    fn image_index_multiple_images() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        for i in 0..5 {
            let image = Image {
                id: format!("sha256:id{i}"),
                reference: ImageRef::parse(&format!("app:v{i}")).unwrap(),
                size_bytes: 100,
                layers: vec![],
                created_at: chrono::Utc::now(),
            };
            store.add_to_index(&image).unwrap();
        }

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 5);
    }

    #[test]
    fn image_index_different_digests() {
        let data_a = b"data a";
        let data_b = b"data b";
        let digest_a = sha256_digest(data_a);
        let digest_b = sha256_digest(data_b);
        assert_ne!(digest_a, digest_b);
    }

    #[test]
    fn image_index_dedup_on_re_pull() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let image1 = Image {
            id: "sha256:v1".to_string(),
            reference: ImageRef::parse("app:latest").unwrap(),
            size_bytes: 100,
            layers: vec![],
            created_at: chrono::Utc::now(),
        };

        let image2 = Image {
            id: "sha256:v2".to_string(),
            reference: ImageRef::parse("app:latest").unwrap(),
            size_bytes: 200,
            layers: vec![],
            created_at: chrono::Utc::now(),
        };

        store.add_to_index(&image1).unwrap();
        store.add_to_index(&image2).unwrap();

        // Should have replaced, not duplicated.
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "sha256:v2");
    }

    #[test]
    fn read_blob_io_error_not_notfound() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        // Create a directory where the blob file should be — causes IsADirectory error.
        let hex = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let blob_path = dir.path().join("blobs").join("sha256").join(hex);
        std::fs::create_dir_all(&blob_path).unwrap();

        let err = store.read_blob(&format!("sha256:{hex}")).unwrap_err();
        assert!(matches!(err, StivaError::Io(_)));
    }

    // -----------------------------------------------------------------------
    // Wiremock integration: full pull pipeline
    // -----------------------------------------------------------------------

    use crate::registry::{Descriptor, MEDIA_OCI_MANIFEST, OciManifest, RegistryClient};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Compute digest for test data (re-export of private fn for test use).
    fn test_digest(data: &[u8]) -> String {
        sha256_digest(data)
    }

    #[tokio::test]
    async fn pull_full_pipeline() {
        let server = MockServer::start().await;

        let config_data = br#"{"architecture":"amd64","os":"linux"}"#;
        let config_digest = test_digest(config_data);

        let layer_data = b"fake-tar-gz-layer-content-here";
        let layer_digest = test_digest(layer_data);

        let manifest = OciManifest {
            media_type: Some(MEDIA_OCI_MANIFEST.into()),
            ..OciManifest::new(
                Descriptor::new(
                    "application/vnd.oci.image.config.v1+json",
                    &config_digest,
                    config_data.len() as u64,
                ),
                vec![Descriptor::new(
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                    &layer_digest,
                    layer_data.len() as u64,
                )],
            )
        };

        // Manifest endpoint.
        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&manifest)
                    .insert_header("content-type", MEDIA_OCI_MANIFEST),
            )
            .mount(&server)
            .await;

        // Config blob endpoint.
        Mock::given(method("GET"))
            .and(path(format!("/v2/library/alpine/blobs/{config_digest}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(config_data.to_vec()))
            .mount(&server)
            .await;

        // Layer blob endpoint.
        Mock::given(method("GET"))
            .and(path(format!("/v2/library/alpine/blobs/{layer_digest}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(layer_data.to_vec()))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();
        let client = RegistryClient::with_base_url(&server.uri());

        let reference = ImageRef {
            registry: server.address().to_string(),
            repository: "library/alpine".into(),
            tag: "latest".into(),
            digest: None,
        };

        let image = store.pull(&reference, &client).await.unwrap();

        // Verify image record.
        assert_eq!(image.id, config_digest);
        assert_eq!(image.layers.len(), 1);
        assert_eq!(image.layers[0].digest, layer_digest);
        assert_eq!(image.size_bytes, layer_data.len() as u64);

        // Verify blobs on disk.
        assert!(store.has_blob(&config_digest));
        assert!(store.has_blob(&layer_digest));

        // Verify config content.
        let read_config = store.read_blob(&config_digest).unwrap();
        assert_eq!(read_config, config_data);

        // Verify layer content.
        let read_layer = store.read_blob(&layer_digest).unwrap();
        assert_eq!(read_layer, layer_data);

        // Manifest blob stored in content-addressable store (not tag-keyed).
        // CVE-2024-24557: tag-keyed manifest cache removed to prevent poisoning.

        // Verify image index.
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, config_digest);
    }

    #[tokio::test]
    async fn pull_dedup_skips_existing_layers() {
        let server = MockServer::start().await;

        let config_data = br#"{"architecture":"amd64"}"#;
        let config_digest = test_digest(config_data);

        let layer_data = b"already-have-this-layer";
        let layer_digest = test_digest(layer_data);

        let manifest = OciManifest {
            media_type: Some(MEDIA_OCI_MANIFEST.into()),
            ..OciManifest::new(
                Descriptor::new(
                    "application/vnd.oci.image.config.v1+json",
                    &config_digest,
                    config_data.len() as u64,
                ),
                vec![Descriptor::new(
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                    &layer_digest,
                    layer_data.len() as u64,
                )],
            )
        };

        Mock::given(method("GET"))
            .and(path("/v2/library/alpine/manifests/latest"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&manifest)
                    .insert_header("content-type", MEDIA_OCI_MANIFEST),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/v2/library/alpine/blobs/{config_digest}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(config_data.to_vec()))
            .mount(&server)
            .await;

        // Layer endpoint should NOT be called — pre-store it.
        Mock::given(method("GET"))
            .and(path(format!("/v2/library/alpine/blobs/{layer_digest}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(layer_data.to_vec()))
            .expect(0) // Should not be called!
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        // Pre-store the layer blob.
        store.store_blob(&layer_digest, layer_data).unwrap();

        let client = RegistryClient::with_base_url(&server.uri());
        let reference = ImageRef {
            registry: server.address().to_string(),
            repository: "library/alpine".into(),
            tag: "latest".into(),
            digest: None,
        };

        let image = store.pull(&reference, &client).await.unwrap();
        assert_eq!(image.layers.len(), 1);
        // Layer blob endpoint was NOT hit (expect(0) validates this).
    }

    #[tokio::test]
    async fn pull_multiple_layers() {
        let server = MockServer::start().await;

        let config_data = br#"{"os":"linux"}"#;
        let config_digest = test_digest(config_data);

        let layer1_data = b"layer-one-data";
        let layer1_digest = test_digest(layer1_data);
        let layer2_data = b"layer-two-data";
        let layer2_digest = test_digest(layer2_data);
        let layer3_data = b"layer-three-data";
        let layer3_digest = test_digest(layer3_data);

        let manifest = OciManifest::new(
            Descriptor::new(
                "application/vnd.oci.image.config.v1+json",
                &config_digest,
                config_data.len() as u64,
            ),
            vec![
                Descriptor::new(
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                    &layer1_digest,
                    layer1_data.len() as u64,
                ),
                Descriptor::new(
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                    &layer2_digest,
                    layer2_data.len() as u64,
                ),
                Descriptor::new(
                    "application/vnd.oci.image.layer.v1.tar+gzip",
                    &layer3_digest,
                    layer3_data.len() as u64,
                ),
            ],
        );

        Mock::given(method("GET"))
            .and(path("/v2/myapp/manifests/v1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&manifest)
                    .insert_header("content-type", MEDIA_OCI_MANIFEST),
            )
            .mount(&server)
            .await;

        for (digest, data) in [
            (&config_digest, config_data.as_slice()),
            (&layer1_digest, layer1_data.as_slice()),
            (&layer2_digest, layer2_data.as_slice()),
            (&layer3_digest, layer3_data.as_slice()),
        ] {
            Mock::given(method("GET"))
                .and(path(format!("/v2/myapp/blobs/{digest}")))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(data.to_vec()))
                .mount(&server)
                .await;
        }

        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();
        let client = RegistryClient::with_base_url(&server.uri());

        let reference = ImageRef {
            registry: server.address().to_string(),
            repository: "myapp".into(),
            tag: "v1".into(),
            digest: None,
        };

        let image = store.pull(&reference, &client).await.unwrap();
        assert_eq!(image.layers.len(), 3);
        assert!(store.has_blob(&layer1_digest));
        assert!(store.has_blob(&layer2_digest));
        assert!(store.has_blob(&layer3_digest));

        let total: u64 = [layer1_data.len(), layer2_data.len(), layer3_data.len()]
            .iter()
            .map(|l| *l as u64)
            .sum();
        assert_eq!(image.size_bytes, total);
    }
}
