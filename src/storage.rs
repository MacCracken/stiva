//! Container storage — overlay filesystem, volumes, tmpfs.

use crate::error::StivaError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Volume mount definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    pub source: PathBuf,
    pub target: PathBuf,
    pub read_only: bool,
}

/// Parse a volume string "source:target[:ro]".
pub fn parse_volume(spec: &str) -> Result<VolumeMount, StivaError> {
    let parts: Vec<&str> = spec.split(':').collect();
    match parts.len() {
        2 => Ok(VolumeMount {
            source: PathBuf::from(parts[0]),
            target: PathBuf::from(parts[1]),
            read_only: false,
        }),
        3 => Ok(VolumeMount {
            source: PathBuf::from(parts[0]),
            target: PathBuf::from(parts[1]),
            read_only: parts[2] == "ro",
        }),
        _ => Err(StivaError::Storage(format!("invalid volume spec: {spec}"))),
    }
}

/// Set up overlay filesystem for a container from image layers.
pub async fn setup_overlay(
    _layers: &[PathBuf],
    _container_root: &Path,
) -> Result<PathBuf, StivaError> {
    // TODO: Create lower/upper/work/merged dirs
    // TODO: Mount overlayfs via nix::mount
    // TODO: Return merged rootfs path
    Err(StivaError::Storage(
        "overlay setup not yet implemented".into(),
    ))
}

/// Tear down overlay filesystem.
pub async fn teardown_overlay(_merged: &Path) -> Result<(), StivaError> {
    // TODO: Unmount overlayfs
    // TODO: Clean up upper/work dirs
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_volume_rw() {
        let vol = parse_volume("/data:/mnt/data").unwrap();
        assert_eq!(vol.source, PathBuf::from("/data"));
        assert_eq!(vol.target, PathBuf::from("/mnt/data"));
        assert!(!vol.read_only);
    }

    #[test]
    fn parse_volume_ro() {
        let vol = parse_volume("/config:/etc/config:ro").unwrap();
        assert!(vol.read_only);
    }

    #[test]
    fn parse_volume_invalid() {
        assert!(parse_volume("nocolon").is_err());
    }

    #[test]
    fn parse_volume_too_many_parts() {
        assert!(parse_volume("/a:/b:ro:extra").is_err());
    }

    #[test]
    fn parse_volume_rw_explicit() {
        let vol = parse_volume("/src:/dst:rw").unwrap();
        // "rw" != "ro", so read_only should be false.
        assert!(!vol.read_only);
    }

    #[test]
    fn volume_mount_serde() {
        let vol = VolumeMount {
            source: PathBuf::from("/host/data"),
            target: PathBuf::from("/container/data"),
            read_only: true,
        };
        let json = serde_json::to_string(&vol).unwrap();
        let back: VolumeMount = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source, PathBuf::from("/host/data"));
        assert!(back.read_only);
    }
}
