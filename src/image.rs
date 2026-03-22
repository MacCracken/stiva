//! OCI image management — pull, store, layer handling.

use crate::error::StivaError;
use crate::registry::RegistryClient;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    pub fn parse(reference: &str) -> Result<Self, StivaError> {
        // Handle digest references (repo@sha256:...)
        let (ref_without_digest, digest) = if let Some((r, d)) = reference.split_once('@') {
            (r, Some(d.to_string()))
        } else {
            (reference, None)
        };

        // Split tag
        let (ref_without_tag, tag) = if let Some((r, t)) = ref_without_digest.rsplit_once(':') {
            // Make sure this isn't a port number
            if r.contains('/') || !t.chars().all(|c| c.is_ascii_digit()) {
                (r, t.to_string())
            } else {
                (ref_without_digest, "latest".to_string())
            }
        } else {
            (ref_without_digest, "latest".to_string())
        };

        // Split registry from repository
        let (registry, repository) =
            if ref_without_tag.contains('.') || ref_without_tag.contains(':') {
                if let Some((reg, repo)) = ref_without_tag.split_once('/') {
                    (reg.to_string(), repo.to_string())
                } else {
                    (
                        "docker.io".to_string(),
                        format!("library/{ref_without_tag}"),
                    )
                }
            } else if ref_without_tag.contains('/') {
                ("docker.io".to_string(), ref_without_tag.to_string())
            } else {
                (
                    "docker.io".to_string(),
                    format!("library/{ref_without_tag}"),
                )
            };

        Ok(Self {
            registry,
            repository,
            tag,
            digest,
        })
    }

    /// Full reference string.
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
        self.store_manifest_ref(reference, &manifest_bytes)?;

        // 3. Download config blob.
        info!(digest = %manifest.config.digest, "pulling config");
        if !self.has_blob(&manifest.config.digest) {
            let config_data = client.fetch_blob(reference, &manifest.config.digest).await?;
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
                async move {
                    info!(digest = %digest, size, "pulling layer");
                    let data = client.fetch_blob(reference, &digest).await?;
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
                size_bytes: d.size,  // Descriptor::size → Layer::size_bytes
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
        let hex = digest_hex(digest);
        let path = self.root.join("blobs").join("sha256").join(&hex);

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
    pub fn has_blob(&self, digest: &str) -> bool {
        let hex = digest_hex(digest);
        self.root
            .join("blobs")
            .join("sha256")
            .join(hex)
            .exists()
    }

    /// Read a blob from the store.
    pub fn read_blob(&self, digest: &str) -> Result<Vec<u8>, StivaError> {
        let hex = digest_hex(digest);
        let path = self.root.join("blobs").join("sha256").join(hex);
        std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StivaError::ImageNotFound(format!("blob {digest} not found"))
            } else {
                StivaError::Io(e)
            }
        })
    }

    // -- Manifest reference storage -----------------------------------------

    /// Store a manifest reference at `manifests/{registry}/{repo}/{tag}.json`.
    fn store_manifest_ref(
        &self,
        reference: &ImageRef,
        manifest_bytes: &[u8],
    ) -> Result<(), StivaError> {
        let dir = self
            .root
            .join("manifests")
            .join(&reference.registry)
            .join(&reference.repository);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", reference.tag));
        std::fs::write(path, manifest_bytes)?;
        Ok(())
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

    fn save_index(&self, images: &[Image]) -> Result<(), StivaError> {
        let data = serde_json::to_vec_pretty(images)?;
        let path = self.index_path();
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn add_to_index(&self, image: &Image) -> Result<(), StivaError> {
        let mut images = self.load_index()?;
        // Replace existing entry for same reference.
        images.retain(|i| i.reference.full_ref() != image.reference.full_ref());
        images.push(image.clone());
        self.save_index(&images)
    }

    /// List locally stored images.
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

    /// Get image storage root.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

// ---------------------------------------------------------------------------
// Digest helpers
// ---------------------------------------------------------------------------

/// Compute `sha256:{hex}` digest for data.
fn sha256_digest(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    format!("sha256:{}", hex::encode(hash))
}

/// Extract the hex portion from a `sha256:{hex}` digest.
fn digest_hex(digest: &str) -> String {
    digest
        .strip_prefix("sha256:")
        .unwrap_or(digest)
        .to_string()
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
        let wrong_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

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
}
