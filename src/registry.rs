//! OCI registry client — pull manifests and blobs from container registries.

use crate::error::StivaError;
use crate::image::ImageRef;
use serde::{Deserialize, Serialize};

/// OCI manifest (simplified).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    #[serde(rename = "mediaType")]
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

/// OCI registry client.
pub struct RegistryClient {
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl RegistryClient {
    /// Create a new registry client.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Fetch manifest for an image.
    pub async fn fetch_manifest(&self, _image: &ImageRef) -> Result<OciManifest, StivaError> {
        // TODO: GET /v2/{repo}/manifests/{tag}
        // TODO: Handle token auth (Docker Hub, GHCR)
        // TODO: Support manifest lists (multi-arch)
        Err(StivaError::Registry("manifest fetch not yet implemented".into()))
    }

    /// Download a blob (layer or config).
    pub async fn fetch_blob(
        &self,
        _image: &ImageRef,
        _digest: &str,
    ) -> Result<bytes::Bytes, StivaError> {
        // TODO: GET /v2/{repo}/blobs/{digest}
        // TODO: Follow redirects (S3, CDN)
        // TODO: Verify digest
        Err(StivaError::Registry("blob fetch not yet implemented".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_client_creation() {
        let _client = RegistryClient::new();
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
}
