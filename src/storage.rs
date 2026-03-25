//! Container storage — overlay filesystem, layer unpacking, volumes, tmpfs.

use crate::error::StivaError;
use crate::image::{ImageStore, Layer};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

// ---------------------------------------------------------------------------
// Volume mounts
// ---------------------------------------------------------------------------

/// Volume mount definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    pub source: PathBuf,
    pub target: PathBuf,
    pub read_only: bool,
}

/// Parse a volume string `"source:target[:ro]"`.
#[must_use = "parsing returns a new VolumeMount"]
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

// ---------------------------------------------------------------------------
// Layer unpacking
// ---------------------------------------------------------------------------

/// Unpack a single tar+gzip layer blob to a directory.
pub fn unpack_layer(blob_path: &Path, dest: &Path) -> Result<(), StivaError> {
    let file = std::fs::File::open(blob_path).map_err(|e| {
        StivaError::LayerUnpack(format!(
            "cannot open layer blob {}: {e}",
            blob_path.display()
        ))
    })?;

    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.set_overwrite(true);
    archive.set_preserve_permissions(true);

    archive.unpack(dest).map_err(|e| {
        StivaError::LayerUnpack(format!(
            "failed to unpack layer {} to {}: {e}",
            blob_path.display(),
            dest.display()
        ))
    })?;

    Ok(())
}

/// Unpack all image layers to the store's `layers/` directory.
///
/// Returns an ordered list of unpacked layer directories (bottom layer first).
/// Layers already unpacked are skipped (dedup by digest).
pub fn prepare_layers(store: &ImageStore, layers: &[Layer]) -> Result<Vec<PathBuf>, StivaError> {
    let mut layer_dirs = Vec::with_capacity(layers.len());

    for layer in layers {
        let hex = layer
            .digest
            .strip_prefix("sha256:")
            .unwrap_or(&layer.digest);
        let layer_dir = store.root().join("layers").join(hex);

        if layer_dir.exists() && layer_dir.join(".unpacked").exists() {
            info!(digest = %layer.digest, "layer already unpacked, skipping");
            layer_dirs.push(layer_dir);
            continue;
        }

        std::fs::create_dir_all(&layer_dir)?;

        let blob_path = store.root().join("blobs").join("sha256").join(hex);
        if !blob_path.exists() {
            return Err(StivaError::LayerUnpack(format!(
                "blob not found for layer {}",
                layer.digest
            )));
        }

        info!(digest = %layer.digest, dest = %layer_dir.display(), "unpacking layer");
        unpack_layer(&blob_path, &layer_dir)?;

        // Mark as unpacked so we skip next time.
        std::fs::write(layer_dir.join(".unpacked"), "")?;

        layer_dirs.push(layer_dir);
    }

    Ok(layer_dirs)
}

// ---------------------------------------------------------------------------
// Overlay filesystem
// ---------------------------------------------------------------------------

/// Overlay directory layout for a container.
#[derive(Debug, Clone)]
pub struct OverlayPaths {
    /// Merged rootfs visible to the container.
    pub merged: PathBuf,
    /// Writable upper layer (container changes).
    pub upper: PathBuf,
    /// Overlayfs work directory.
    pub work: PathBuf,
    /// Container root directory (parent of all overlay dirs).
    pub container_root: PathBuf,
}

/// Prepare overlay directory structure and mount overlayfs.
///
/// `layers` are ordered bottom-to-top (first = lowest layer).
/// On non-Linux systems, returns an error since overlayfs is Linux-only.
pub fn setup_overlay(
    layers: &[PathBuf],
    container_root: &Path,
) -> Result<OverlayPaths, StivaError> {
    if layers.is_empty() {
        return Err(StivaError::Overlay("no layers provided".into()));
    }

    let upper = container_root.join("upper");
    let work = container_root.join("work");
    let merged = container_root.join("merged");

    std::fs::create_dir_all(&upper)?;
    std::fs::create_dir_all(&work)?;
    std::fs::create_dir_all(&merged)?;

    #[cfg(target_os = "linux")]
    {
        // Build lowerdir string: layers in reverse order (top layer first for overlayfs).
        let lowerdir: String = layers
            .iter()
            .rev()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(":");

        let opts = format!(
            "lowerdir={},upperdir={},workdir={}",
            lowerdir,
            upper.display(),
            work.display()
        );

        nix::mount::mount(
            Some("overlay"),
            &merged,
            Some("overlay"),
            nix::mount::MsFlags::empty(),
            Some(opts.as_str()),
        )
        .map_err(|e| StivaError::Overlay(format!("mount overlay failed: {e}")))?;

        info!(merged = %merged.display(), "overlay mounted");
    }

    #[cfg(not(target_os = "linux"))]
    {
        return Err(StivaError::Overlay("overlayfs requires Linux".into()));
    }

    Ok(OverlayPaths {
        merged,
        upper,
        work,
        container_root: container_root.to_path_buf(),
    })
}

/// Tear down overlay filesystem — unmount and clean up.
pub fn teardown_overlay(paths: &OverlayPaths) -> Result<(), StivaError> {
    #[cfg(target_os = "linux")]
    {
        // Unmount merged directory. MNT_DETACH allows lazy unmount if busy.
        if paths.merged.exists()
            && let Err(e) = nix::mount::umount2(&paths.merged, nix::mount::MntFlags::MNT_DETACH)
        {
            tracing::warn!(path = %paths.merged.display(), "overlay umount failed: {e}");
        }
    }

    // Clean up writable layers.
    if paths.upper.exists() {
        let _ = std::fs::remove_dir_all(&paths.upper);
    }
    if paths.work.exists() {
        let _ = std::fs::remove_dir_all(&paths.work);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Volume mounting
// ---------------------------------------------------------------------------

/// Mount bind volumes into the container rootfs.
///
/// Each volume spec is `"source:target[:ro]"`. The target path is relative to
/// the merged rootfs directory.
#[cfg(target_os = "linux")]
pub fn mount_volumes(
    volume_specs: &[String],
    merged_rootfs: &Path,
) -> Result<Vec<VolumeMount>, StivaError> {
    let mut mounts = Vec::new();

    for spec in volume_specs {
        let vol = parse_volume(spec)?;
        let target_in_rootfs =
            merged_rootfs.join(vol.target.strip_prefix("/").unwrap_or(&vol.target));

        std::fs::create_dir_all(&target_in_rootfs)?;

        let mut flags = nix::mount::MsFlags::MS_BIND;
        if vol.read_only {
            flags |= nix::mount::MsFlags::MS_RDONLY;
        }

        nix::mount::mount(
            Some(vol.source.as_path()),
            &target_in_rootfs,
            None::<&str>,
            flags,
            None::<&str>,
        )
        .map_err(|e| {
            StivaError::Storage(format!(
                "bind mount {} → {} failed: {e}",
                vol.source.display(),
                target_in_rootfs.display()
            ))
        })?;

        mounts.push(vol);
    }

    Ok(mounts)
}

/// Stub for non-Linux: volume mounting is not supported.
#[cfg(not(target_os = "linux"))]
pub fn mount_volumes(
    volume_specs: &[String],
    _merged_rootfs: &Path,
) -> Result<Vec<VolumeMount>, StivaError> {
    if !volume_specs.is_empty() {
        return Err(StivaError::Storage("bind mounts require Linux".into()));
    }
    Ok(vec![])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
        assert!(!vol.read_only);
    }

    #[test]
    fn parse_named_volume() {
        let vol = parse_volume("pgdata:/var/lib/postgresql/data").unwrap();
        assert_eq!(vol.source, PathBuf::from("pgdata"));
        assert_eq!(vol.target, PathBuf::from("/var/lib/postgresql/data"));
        assert!(!vol.read_only);
    }

    #[test]
    fn parse_volume_empty_string() {
        assert!(parse_volume("").is_err());
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

    // -- Layer unpacking tests --

    #[test]
    fn unpack_layer_from_tar_gz() {
        let dir = tempfile::tempdir().unwrap();

        // Create a tar.gz with a file inside.
        let tar_gz_path = dir.path().join("layer.tar.gz");
        {
            let file = std::fs::File::create(&tar_gz_path).unwrap();
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            let mut builder = tar::Builder::new(enc);

            let data = b"hello from layer";
            let mut header = tar::Header::new_gnu();
            header.set_path("etc/hello.txt").unwrap();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &data[..]).unwrap();
            builder.finish().unwrap();
        }

        let dest = dir.path().join("unpacked");
        unpack_layer(&tar_gz_path, &dest).unwrap();

        let content = std::fs::read_to_string(dest.join("etc/hello.txt")).unwrap();
        assert_eq!(content, "hello from layer");
    }

    #[test]
    fn unpack_layer_missing_blob() {
        let err = unpack_layer(Path::new("/nonexistent/blob"), Path::new("/tmp"));
        assert!(err.is_err());
    }

    #[test]
    fn unpack_layer_not_gzip() {
        let dir = tempfile::tempdir().unwrap();
        let bad_path = dir.path().join("bad.tar.gz");
        std::fs::write(&bad_path, b"this is not gzip").unwrap();

        let dest = dir.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let err = unpack_layer(&bad_path, &dest);
        assert!(err.is_err());
    }

    #[test]
    fn prepare_layers_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        // Create a tar.gz blob.
        let data = b"layer content";
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest;
        hasher.update(data);

        // Build a real tar.gz.
        let blob_data = {
            let mut buf = Vec::new();
            {
                let enc = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::fast());
                let mut builder = tar::Builder::new(enc);
                let mut header = tar::Header::new_gnu();
                header.set_path("file.txt").unwrap();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder.append(&header, &data[..]).unwrap();
                builder.finish().unwrap();
            }
            buf
        };

        // Store it as a real blob with correct digest.
        let digest = {
            let mut h = sha2::Sha256::new();
            h.update(&blob_data);
            format!("sha256:{}", hex::encode(h.finalize()))
        };
        store.store_blob(&digest, &blob_data).unwrap();

        let layer = Layer {
            digest: digest.clone(),
            size_bytes: blob_data.len() as u64,
            media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
        };

        // First prepare: unpacks.
        let dirs = prepare_layers(&store, std::slice::from_ref(&layer)).unwrap();
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].join("file.txt").exists());
        assert!(dirs[0].join(".unpacked").exists());

        // Second prepare: dedup (skips unpack).
        let dirs2 = prepare_layers(&store, &[layer]).unwrap();
        assert_eq!(dirs2.len(), 1);
        assert_eq!(dirs[0], dirs2[0]);
    }

    #[test]
    fn prepare_layers_missing_blob() {
        let dir = tempfile::tempdir().unwrap();
        let store = ImageStore::new(dir.path()).unwrap();

        let layer = Layer {
            digest: "sha256:doesnotexist".into(),
            size_bytes: 0,
            media_type: "application/vnd.oci.image.layer.v1.tar+gzip".into(),
        };

        let err = prepare_layers(&store, &[layer]).unwrap_err();
        assert!(matches!(err, StivaError::LayerUnpack(_)));
    }

    // -- Overlay tests --

    #[test]
    fn setup_overlay_no_layers() {
        let dir = tempfile::tempdir().unwrap();
        let err = setup_overlay(&[], dir.path()).unwrap_err();
        assert!(matches!(err, StivaError::Overlay(_)));
    }

    #[test]
    fn setup_overlay_creates_directories() {
        // This tests the directory creation path. The actual mount will fail
        // without root, but the dirs should be created.
        let dir = tempfile::tempdir().unwrap();
        let layer_dir = dir.path().join("layer0");
        std::fs::create_dir_all(&layer_dir).unwrap();

        let container_root = dir.path().join("container");
        std::fs::create_dir_all(&container_root).unwrap();

        // On Linux without root, mount will fail — that's expected.
        // We're testing the directory setup + error path.
        let result = setup_overlay(&[layer_dir], &container_root);

        // Directories should have been created regardless of mount success.
        assert!(container_root.join("upper").exists());
        assert!(container_root.join("work").exists());
        assert!(container_root.join("merged").exists());

        // Mount fails without root on Linux, or platform error on non-Linux.
        if let Err(err) = result {
            assert!(matches!(err, StivaError::Overlay(_)));
        }
    }

    #[test]
    fn teardown_overlay_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let paths = OverlayPaths {
            merged: dir.path().join("merged"),
            upper: dir.path().join("upper"),
            work: dir.path().join("work"),
            container_root: dir.path().to_path_buf(),
        };

        std::fs::create_dir_all(&paths.upper).unwrap();
        std::fs::create_dir_all(&paths.work).unwrap();
        std::fs::create_dir_all(&paths.merged).unwrap();

        teardown_overlay(&paths).unwrap();

        assert!(!paths.upper.exists());
        assert!(!paths.work.exists());
    }

    #[test]
    fn overlay_paths_debug() {
        let paths = OverlayPaths {
            merged: PathBuf::from("/merged"),
            upper: PathBuf::from("/upper"),
            work: PathBuf::from("/work"),
            container_root: PathBuf::from("/root"),
        };
        let dbg = format!("{paths:?}");
        assert!(dbg.contains("merged"));
    }

    // -- mount_volumes tests --

    #[test]
    fn mount_volumes_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mounts = mount_volumes(&[], dir.path()).unwrap();
        assert!(mounts.is_empty());
    }
}
