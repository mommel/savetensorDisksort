//! Physical filesystem walker across mountpoints (ignoring symlinks).

use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::file_info::{FileInfo, MountpointInfo};
use super::filters::{detect_category, is_model_extension, should_exclude_path};
use crate::utils::{
    canonicalize_lossy, find_matching_mountpoint, format_bytes, get_relative_path, normalize_path,
};

/// Scan a single mountpoint directory for physical model files.
/// Symlinks are strictly skipped.
pub fn scan_mountpoint<P: AsRef<Path>>(
    mountpoint_path: P,
    mountpoints: &[PathBuf],
) -> Vec<FileInfo> {
    let mp_path = mountpoint_path.as_ref();
    if !mp_path.exists() || !mp_path.is_dir() {
        log::warn!(
            "Mountpoint '{}' does not exist or is not a directory",
            mp_path.display()
        );
        return Vec::new();
    }

    let mp_canon = canonicalize_lossy(mp_path);
    let mut files = Vec::new();

    // Walk directory without following symlinks
    for entry_res in WalkDir::new(&mp_canon).follow_links(false).into_iter() {
        let entry = match entry_res {
            Ok(e) => e,
            Err(err) => {
                log::warn!("Permission or I/O error during scan: {}", err);
                continue;
            }
        };

        // Skip symlinks
        if entry.file_type().is_symlink() {
            continue;
        }

        // Only process regular files
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        if should_exclude_path(path) {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if !is_model_extension(&ext) {
            continue;
        }

        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("Failed to read metadata for {}: {}", path.display(), e);
                continue;
            }
        };

        let size_bytes = metadata.len();
        let modified_at: Option<DateTime<Utc>> =
            metadata.modified().ok().map(|st| DateTime::<Utc>::from(st));

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let matched_mp = find_matching_mountpoint(path, mountpoints).unwrap_or(&mp_canon);
        let rel_path = get_relative_path(path, matched_mp);
        let category = detect_category(path, size_bytes);

        files.push(FileInfo {
            id: String::new(), // Assigned globally later
            real_path: normalize_path(path),
            filename,
            extension: ext,
            category,
            size_bytes,
            size_human: format_bytes(size_bytes),
            blake3_hash: None,
            modified_at,
            mountpoint: normalize_path(matched_mp),
            relative_path: normalize_path(rel_path),
            symlinked_from: Vec::new(),
        });
    }

    files
}

/// Parallel scan across all configured mountpoints.
/// Assigns sequential unique IDs (f-0001, f-0002, ...).
pub fn scan_all_mountpoints(mountpoints: &[PathBuf]) -> Vec<FileInfo> {
    let mut all_files: Vec<FileInfo> = mountpoints
        .par_iter()
        .flat_map(|mp| scan_mountpoint(mp, mountpoints))
        .collect();

    // Deduplicate any accidental duplicate real paths and sort deterministically
    all_files.sort_by(|a, b| a.real_path.cmp(&b.real_path));
    all_files.dedup_by(|a, b| a.real_path == b.real_path);

    // Assign sequential IDs: f-0001, f-0002, ...
    for (i, file) in all_files.iter_mut().enumerate() {
        file.id = format!("f-{:04}", i + 1);
    }

    all_files
}

/// Query filesystem metadata (total bytes, free bytes, fs type) for a mountpoint.
pub fn inspect_mountpoint<P: AsRef<Path>>(path: P, label: Option<&str>) -> MountpointInfo {
    let p = path.as_ref();
    let norm_path = normalize_path(p);
    let lbl = label.map(|s| s.to_string()).unwrap_or_else(|| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&norm_path)
            .to_string()
    });

    let mut total_bytes = 0u64;
    let mut free_bytes = 0u64;
    let mut fs_type = "unknown".to_string();

    let mut sys = sysinfo::Disks::new_with_refreshed_list();
    sys.refresh(false);

    let canon_p = canonicalize_lossy(p);
    for disk in sys.list() {
        let disk_mp = canonicalize_lossy(disk.mount_point());
        if canon_p.starts_with(&disk_mp) {
            total_bytes = disk.total_space();
            free_bytes = disk.available_space();
            fs_type = disk.file_system().to_string_lossy().to_string();
            break;
        }
    }

    MountpointInfo {
        path: norm_path,
        label: lbl,
        total_bytes,
        free_bytes,
        filesystem: fs_type,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_scan_empty_mountpoint() {
        let dir = tempdir().unwrap();
        let mps = vec![dir.path().to_path_buf()];
        let results = scan_all_mountpoints(&mps);
        assert!(results.is_empty());
    }

    #[test]
    fn test_scan_nested_structure() {
        let dir = tempdir().unwrap();
        let mp = dir.path().join("drive_a");
        let sd_dir = mp.join("models").join("sd");
        let lora_dir = mp.join("models").join("lora");

        fs::create_dir_all(&sd_dir).unwrap();
        fs::create_dir_all(&lora_dir).unwrap();

        let f1 = sd_dir.join("v1-5-pruned.safetensors");
        let f2 = lora_dir.join("detail.safetensors");
        let f3_ignored = sd_dir.join("readme.txt");

        fs::write(&f1, b"checkpoint data 12345").unwrap();
        fs::write(&f2, b"lora data").unwrap();
        fs::write(&f3_ignored, b"hello world").unwrap();

        let mps = vec![mp.clone()];
        let files = scan_all_mountpoints(&mps);

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].id, "f-0001");
        assert_eq!(files[1].id, "f-0002");
        assert!(files
            .iter()
            .any(|f| f.filename == "v1-5-pruned.safetensors"));
        assert!(files.iter().any(|f| f.filename == "detail.safetensors"));
    }
}
