//! Encrypted and verified container storage.
//!
//! Provides LUKS-encrypted volumes and dm-verity integrity verification for
//! container rootfs and data volumes. With the `encrypted` feature enabled,
//! delegates to agnosys for `cryptsetup` and `veritysetup` operations.
//! Without it, all functions return [`StivaError`].

use crate::error::StivaError;
use std::path::{Path, PathBuf};

/// LUKS volume configuration for a container.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuksVolumeConfig {
    /// Backing image path (e.g., `/var/lib/agnos/volumes/agent-data.img`).
    pub image_path: PathBuf,
    /// Mapper name (e.g., `stiva-agent-data`).
    pub mapper_name: String,
    /// Mount point inside the container root.
    pub mount_point: PathBuf,
    /// Volume size in MB (for format).
    pub size_mb: u64,
}

/// dm-verity configuration for a verified rootfs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerityVolumeConfig {
    /// Data device or image path.
    pub data_path: PathBuf,
    /// Hash device or image path.
    pub hash_path: PathBuf,
    /// Expected root hash (hex string).
    pub root_hash: String,
    /// Mapper name (e.g., `stiva-rootfs-verified`).
    pub mapper_name: String,
}

/// Result of opening an encrypted or verified volume.
#[derive(Debug, Clone)]
pub struct OpenVolume {
    /// Device-mapper path (e.g., `/dev/mapper/stiva-agent-data`).
    pub dm_path: PathBuf,
    /// Mapper name used.
    pub mapper_name: String,
}

/// Open a LUKS-encrypted volume for a container.
///
/// Requires the `encrypted` feature, `cryptsetup`, and root privileges.
pub fn luks_open(config: &LuksVolumeConfig, passphrase: &str) -> Result<OpenVolume, StivaError> {
    #[cfg(feature = "encrypted")]
    {
        let mut luks_config = agnosys::luks::LuksConfig::default();
        luks_config.name = config.mapper_name.clone();
        luks_config.backing_path = config.image_path.clone();
        luks_config.size_mb = config.size_mb;
        luks_config.mount_point = config.mount_point.clone();
        let key = agnosys::luks::LuksKey::from_passphrase(passphrase)
            .map_err(|e| StivaError::Storage(format!("invalid passphrase: {e}")))?;
        let dm_path = agnosys::luks::luks_open(&luks_config, &key)
            .map_err(|e| StivaError::Storage(format!("LUKS open failed: {e}")))?;
        Ok(OpenVolume {
            dm_path,
            mapper_name: config.mapper_name.clone(),
        })
    }
    #[cfg(not(feature = "encrypted"))]
    {
        let _ = (config, passphrase);
        Err(StivaError::Storage(
            "encrypted storage requires the 'encrypted' feature".into(),
        ))
    }
}

/// Close a LUKS-encrypted volume.
pub fn luks_close(mapper_name: &str) -> Result<(), StivaError> {
    #[cfg(feature = "encrypted")]
    {
        agnosys::luks::luks_close(mapper_name)
            .map_err(|e| StivaError::Storage(format!("LUKS close failed: {e}")))
    }
    #[cfg(not(feature = "encrypted"))]
    {
        let _ = mapper_name;
        Err(StivaError::Storage(
            "encrypted storage requires the 'encrypted' feature".into(),
        ))
    }
}

/// Format a backing image as a LUKS2 volume.
///
/// Creates a new encrypted volume. Requires root privileges.
pub fn luks_format(config: &LuksVolumeConfig, passphrase: &str) -> Result<PathBuf, StivaError> {
    #[cfg(feature = "encrypted")]
    {
        let mut luks_config = agnosys::luks::LuksConfig::default();
        luks_config.name = config.mapper_name.clone();
        luks_config.backing_path = config.image_path.clone();
        luks_config.size_mb = config.size_mb;
        luks_config.mount_point = config.mount_point.clone();
        let key = agnosys::luks::LuksKey::from_passphrase(passphrase)
            .map_err(|e| StivaError::Storage(format!("invalid passphrase: {e}")))?;
        agnosys::luks::luks_format(&luks_config, &key)
            .map_err(|e| StivaError::Storage(format!("LUKS format failed: {e}")))
    }
    #[cfg(not(feature = "encrypted"))]
    {
        let _ = (config, passphrase);
        Err(StivaError::Storage(
            "encrypted storage requires the 'encrypted' feature".into(),
        ))
    }
}

/// Check if `cryptsetup` is available on the system.
#[must_use]
pub fn cryptsetup_available() -> bool {
    #[cfg(feature = "encrypted")]
    {
        agnosys::luks::cryptsetup_available()
    }
    #[cfg(not(feature = "encrypted"))]
    {
        false
    }
}

/// Open a dm-verity verified volume.
///
/// Requires the `encrypted` feature, `veritysetup`, and root privileges.
pub fn verity_open(config: &VerityVolumeConfig) -> Result<OpenVolume, StivaError> {
    #[cfg(feature = "encrypted")]
    {
        let verity_config = make_verity_config(config)?;
        agnosys::dmverity::verity_open(&verity_config)
            .map_err(|e| StivaError::Storage(format!("dm-verity open failed: {e}")))?;
        Ok(OpenVolume {
            dm_path: PathBuf::from(format!("/dev/mapper/{}", config.mapper_name)),
            mapper_name: config.mapper_name.clone(),
        })
    }
    #[cfg(not(feature = "encrypted"))]
    {
        let _ = config;
        Err(StivaError::Storage(
            "verified storage requires the 'encrypted' feature".into(),
        ))
    }
}

/// Close a dm-verity verified volume.
pub fn verity_close(mapper_name: &str) -> Result<(), StivaError> {
    #[cfg(feature = "encrypted")]
    {
        agnosys::dmverity::verity_close(mapper_name)
            .map_err(|e| StivaError::Storage(format!("dm-verity close failed: {e}")))
    }
    #[cfg(not(feature = "encrypted"))]
    {
        let _ = mapper_name;
        Err(StivaError::Storage(
            "verified storage requires the 'encrypted' feature".into(),
        ))
    }
}

/// Generate dm-verity hash tree and root hash for a data image.
pub fn verity_format(data_path: &Path, hash_path: &Path) -> Result<String, StivaError> {
    #[cfg(feature = "encrypted")]
    {
        agnosys::dmverity::verity_format(
            data_path,
            hash_path,
            agnosys::dmverity::VerityHashAlgorithm::Sha256,
            None,
        )
        .map_err(|e| StivaError::Storage(format!("dm-verity format failed: {e}")))
    }
    #[cfg(not(feature = "encrypted"))]
    {
        let _ = (data_path, hash_path);
        Err(StivaError::Storage(
            "verified storage requires the 'encrypted' feature".into(),
        ))
    }
}

/// Check if `veritysetup` is available on the system.
#[must_use]
pub fn veritysetup_available() -> bool {
    #[cfg(feature = "encrypted")]
    {
        agnosys::dmverity::verity_supported()
    }
    #[cfg(not(feature = "encrypted"))]
    {
        false
    }
}

/// Build a VerityConfig from our VerityVolumeConfig.
/// Works around agnosys 0.50.0 #[non_exhaustive] on VerityConfig.
#[cfg(feature = "encrypted")]
fn make_verity_config(
    config: &VerityVolumeConfig,
) -> Result<agnosys::dmverity::VerityConfig, StivaError> {
    // Use serde round-trip to construct non_exhaustive struct.
    let json = serde_json::json!({
        "name": config.mapper_name,
        "data_device": config.data_path,
        "hash_device": config.hash_path,
        "data_block_size": 4096,
        "hash_block_size": 4096,
        "hash_algorithm": "Sha256",
        "root_hash": config.root_hash,
        "salt": null,
    });
    serde_json::from_value(json)
        .map_err(|e| StivaError::Storage(format!("failed to construct VerityConfig: {e}")))
}

// ---------------------------------------------------------------------------
// OCI Image Layer Encryption
// ---------------------------------------------------------------------------

/// Key material source for OCI image layer encryption/decryption.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum KeySource {
    /// Path to a key file (raw 32-byte AES-256 key).
    File(PathBuf),
    /// Environment variable name containing hex-encoded key.
    EnvVar(String),
}

/// Check if an OCI media type indicates an encrypted layer.
#[must_use]
#[inline]
pub fn is_encrypted_media_type(media_type: &str) -> bool {
    media_type.ends_with("+encrypted")
}

/// Strip the `+encrypted` suffix from a media type.
#[must_use]
#[inline]
pub fn strip_encrypted_suffix(media_type: &str) -> &str {
    media_type.strip_suffix("+encrypted").unwrap_or(media_type)
}

/// Load raw key bytes from a KeySource.
pub fn load_key(source: &KeySource) -> Result<Vec<u8>, StivaError> {
    match source {
        KeySource::File(path) => std::fs::read(path).map_err(|e| {
            StivaError::Encryption(format!("failed to read key file {}: {e}", path.display()))
        }),
        KeySource::EnvVar(var) => {
            let hex = std::env::var(var)
                .map_err(|e| StivaError::Encryption(format!("env var {var} not set: {e}")))?;
            hex::decode(hex.trim())
                .map_err(|e| StivaError::Encryption(format!("invalid hex key in {var}: {e}")))
        }
    }
}

/// Decrypt an encrypted OCI image layer.
///
/// Uses AES-256-GCM. The first 12 bytes of `data` are the nonce,
/// the remaining bytes are the ciphertext + tag.
#[must_use = "decrypted layer data must be used"]
#[cfg(feature = "encrypted")]
pub fn decrypt_layer(data: &[u8], key_source: &KeySource) -> Result<Vec<u8>, StivaError> {
    use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};

    let raw_key = load_key(key_source)?;
    if raw_key.len() != 32 {
        return Err(StivaError::Encryption(format!(
            "key must be 32 bytes, got {}",
            raw_key.len()
        )));
    }

    if data.len() < 12 {
        return Err(StivaError::Encryption("ciphertext too short".into()));
    }

    let nonce = aes_gcm::Nonce::from_slice(&data[..12]);
    let cipher = Aes256Gcm::new_from_slice(&raw_key)
        .map_err(|e| StivaError::Encryption(format!("invalid key: {e}")))?;
    cipher
        .decrypt(nonce, &data[12..])
        .map_err(|e| StivaError::Encryption(format!("decryption failed: {e}")))
}

/// Encrypt data as an OCI image layer.
///
/// Uses AES-256-GCM. Returns nonce (12 bytes) || ciphertext || tag.
#[must_use = "encrypted layer data must be used"]
#[cfg(feature = "encrypted")]
pub fn encrypt_layer(data: &[u8], key_source: &KeySource) -> Result<Vec<u8>, StivaError> {
    use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};

    let raw_key = load_key(key_source)?;
    if raw_key.len() != 32 {
        return Err(StivaError::Encryption(format!(
            "key must be 32 bytes, got {}",
            raw_key.len()
        )));
    }

    let nonce = aes_gcm::Nonce::from(rand_nonce()?);
    let cipher = Aes256Gcm::new_from_slice(&raw_key)
        .map_err(|e| StivaError::Encryption(format!("invalid key: {e}")))?;
    let ciphertext = cipher
        .encrypt(&nonce, data)
        .map_err(|e| StivaError::Encryption(format!("encryption failed: {e}")))?;

    let mut result = Vec::with_capacity(12 + ciphertext.len());
    result.extend_from_slice(&nonce);
    result.extend(ciphertext);
    Ok(result)
}

#[cfg(not(feature = "encrypted"))]
pub fn decrypt_layer(_data: &[u8], _key_source: &KeySource) -> Result<Vec<u8>, StivaError> {
    Err(StivaError::Encryption(
        "image encryption requires the 'encrypted' feature".into(),
    ))
}

#[cfg(not(feature = "encrypted"))]
pub fn encrypt_layer(_data: &[u8], _key_source: &KeySource) -> Result<Vec<u8>, StivaError> {
    Err(StivaError::Encryption(
        "image encryption requires the 'encrypted' feature".into(),
    ))
}

/// Generate a random 12-byte nonce.
#[cfg(feature = "encrypted")]
fn rand_nonce() -> Result<[u8; 12], StivaError> {
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce)
        .map_err(|e| StivaError::Encryption(format!("failed to generate random nonce: {e}")))?;
    Ok(nonce)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_media_type_detection() {
        assert!(is_encrypted_media_type(
            "application/vnd.oci.image.layer.v1.tar+gzip+encrypted"
        ));
        assert!(!is_encrypted_media_type(
            "application/vnd.oci.image.layer.v1.tar+gzip"
        ));
    }

    #[test]
    fn strip_encrypted_suffix_works() {
        assert_eq!(
            strip_encrypted_suffix("application/vnd.oci.image.layer.v1.tar+gzip+encrypted"),
            "application/vnd.oci.image.layer.v1.tar+gzip"
        );
        assert_eq!(
            strip_encrypted_suffix("application/vnd.oci.image.layer.v1.tar+gzip"),
            "application/vnd.oci.image.layer.v1.tar+gzip"
        );
    }

    #[test]
    fn load_key_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("test.key");
        let key_data = vec![0x42u8; 32];
        std::fs::write(&key_path, &key_data).unwrap();

        let loaded = load_key(&KeySource::File(key_path)).unwrap();
        assert_eq!(loaded, key_data);
    }

    #[test]
    fn load_key_from_env() {
        let key_hex = "aa".repeat(32);
        // SAFETY: test-only, single-threaded access to this unique var name.
        unsafe { std::env::set_var("STIVA_TEST_KEY_12345", &key_hex) };
        let loaded = load_key(&KeySource::EnvVar("STIVA_TEST_KEY_12345".into())).unwrap();
        assert_eq!(loaded.len(), 32);
        unsafe { std::env::remove_var("STIVA_TEST_KEY_12345") };
    }

    #[test]
    fn load_key_missing_file() {
        assert!(load_key(&KeySource::File("/nonexistent/key".into())).is_err());
    }

    #[test]
    fn load_key_missing_env() {
        assert!(load_key(&KeySource::EnvVar("STIVA_NONEXISTENT_KEY_VAR".into())).is_err());
    }

    #[test]
    fn luks_config_serde() {
        let config = LuksVolumeConfig {
            image_path: "/var/lib/agnos/volumes/data.img".into(),
            mapper_name: "stiva-data".into(),
            mount_point: "/data".into(),
            size_mb: 512,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: LuksVolumeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mapper_name, "stiva-data");
        assert_eq!(back.size_mb, 512);
    }

    #[test]
    fn verity_config_serde() {
        let config = VerityVolumeConfig {
            data_path: "/var/lib/agnos/rootfs.img".into(),
            hash_path: "/var/lib/agnos/rootfs.hash".into(),
            root_hash: "abc123".into(),
            mapper_name: "stiva-rootfs".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: VerityVolumeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.root_hash, "abc123");
    }

    #[test]
    fn open_volume_debug() {
        let vol = OpenVolume {
            dm_path: "/dev/mapper/stiva-data".into(),
            mapper_name: "stiva-data".into(),
        };
        let dbg = format!("{:?}", vol);
        assert!(dbg.contains("stiva-data"));
    }

    #[test]
    fn luks_open_without_feature_or_device() {
        let config = LuksVolumeConfig {
            image_path: "/nonexistent/volume.img".into(),
            mapper_name: "test-vol".into(),
            mount_point: "/data".into(),
            size_mb: 64,
        };
        assert!(luks_open(&config, "password123").is_err());
    }

    #[test]
    fn luks_close_nonexistent() {
        // May succeed (no-op) or error depending on cryptsetup availability
        let _ = luks_close("nonexistent-mapper");
    }

    #[test]
    fn verity_open_nonexistent() {
        let config = VerityVolumeConfig {
            data_path: "/nonexistent/data.img".into(),
            hash_path: "/nonexistent/hash.img".into(),
            root_hash: "abc".into(),
            mapper_name: "test-verity".into(),
        };
        assert!(verity_open(&config).is_err());
    }

    #[test]
    fn verity_close_nonexistent() {
        assert!(verity_close("nonexistent-mapper").is_err());
    }

    #[test]
    fn cryptsetup_available_check() {
        let _ = cryptsetup_available();
    }

    #[test]
    fn veritysetup_available_check() {
        let _ = veritysetup_available();
    }

    #[test]
    fn luks_format_nonexistent() {
        let config = LuksVolumeConfig {
            image_path: "/nonexistent/img".into(),
            mapper_name: "test".into(),
            mount_point: "/mnt".into(),
            size_mb: 64,
        };
        assert!(luks_format(&config, "password123").is_err());
    }

    #[test]
    fn verity_format_nonexistent() {
        assert!(
            verity_format(
                Path::new("/nonexistent/data"),
                Path::new("/nonexistent/hash")
            )
            .is_err()
        );
    }
}
