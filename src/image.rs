//! OCI image management — pull, store, layer handling.

use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
        let (registry, repository) = if ref_without_tag.contains('.') || ref_without_tag.contains(':') {
            if let Some((reg, repo)) = ref_without_tag.split_once('/') {
                (reg.to_string(), repo.to_string())
            } else {
                ("docker.io".to_string(), format!("library/{ref_without_tag}"))
            }
        } else if ref_without_tag.contains('/') {
            ("docker.io".to_string(), ref_without_tag.to_string())
        } else {
            ("docker.io".to_string(), format!("library/{ref_without_tag}"))
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

/// Local image storage.
pub struct ImageStore {
    root: PathBuf,
}

impl ImageStore {
    /// Open or create an image store.
    pub fn new(root: &Path) -> Result<Self, StivaError> {
        std::fs::create_dir_all(root)?;
        std::fs::create_dir_all(root.join("blobs"))?;
        std::fs::create_dir_all(root.join("manifests"))?;
        std::fs::create_dir_all(root.join("layers"))?;
        Ok(Self { root: root.to_path_buf() })
    }

    /// Pull an image from a registry.
    pub async fn pull(&self, _reference: &ImageRef) -> Result<Image, StivaError> {
        // TODO: OCI distribution spec — fetch manifest, download layers, assemble
        Err(StivaError::PullFailed("image pull not yet implemented".into()))
    }

    /// List locally stored images.
    pub fn list(&self) -> Result<Vec<Image>, StivaError> {
        // TODO: Read manifest index
        Ok(vec![])
    }

    /// Remove an image.
    pub fn remove(&self, _id: &str) -> Result<(), StivaError> {
        // TODO: Remove manifests + unreferenced layer blobs
        Ok(())
    }

    /// Get image storage root.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

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
        assert!(store.root().join("blobs").exists());
        assert!(store.root().join("manifests").exists());
        assert!(store.root().join("layers").exists());
    }
}
