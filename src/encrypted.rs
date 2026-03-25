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
        let luks_config = agnosys::luks::LuksConfig {
            name: config.mapper_name.clone(),
            backing_path: config.image_path.clone(),
            size_mb: config.size_mb,
            mount_point: config.mount_point.clone(),
            ..Default::default()
        };
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
        let luks_config = agnosys::luks::LuksConfig {
            name: config.mapper_name.clone(),
            backing_path: config.image_path.clone(),
            size_mb: config.size_mb,
            mount_point: config.mount_point.clone(),
            ..Default::default()
        };
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
        let verity_config = agnosys::dmverity::VerityConfig {
            name: config.mapper_name.clone(),
            data_device: config.data_path.clone(),
            hash_device: config.hash_path.clone(),
            data_block_size: 4096,
            hash_block_size: 4096,
            hash_algorithm: agnosys::dmverity::VerityHashAlgorithm::Sha256,
            root_hash: config.root_hash.clone(),
            salt: None,
        };
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

#[cfg(test)]
mod tests {
    use super::*;

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
