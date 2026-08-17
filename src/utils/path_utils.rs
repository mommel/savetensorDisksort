//! Cross-platform path helpers, mountpoint detection, and normalization.

use std::fs;
use std::path::{Component, Path, PathBuf};

/// Normalize a path to use forward slashes and strip Windows `\\?\` UNC prefixes where appropriate.
pub fn normalize_path<P: AsRef<Path>>(path: P) -> String {
    let p_str = path.as_ref().to_string_lossy();
    let cleaned = if let Some(stripped) = p_str.strip_prefix(r"\\?\") {
        stripped
    } else {
        &p_str
    };
    cleaned.replace('\\', "/")
}

/// Lossy canonicalize that resolves symlinks if the file exists, or cleans components if it does not.
pub fn canonicalize_lossy<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();
    if let Ok(canon) = fs::canonicalize(path) {
        // Strip Windows UNC prefix if present
        let s = canon.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            PathBuf::from(stripped)
        } else {
            canon
        }
    } else {
        // Resolve '.' and '..' statically
        let mut out = PathBuf::new();
        for comp in path.components() {
            match comp {
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                _ => out.push(comp),
            }
        }
        out
    }
}

/// Check if a path entry is a symlink.
pub fn is_symlink<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref();
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Given a file path and a list of mountpoint paths, find the mountpoint with the longest matching prefix.
pub fn find_matching_mountpoint<'a, P: AsRef<Path>>(
    file_path: P,
    mountpoints: &'a [PathBuf],
) -> Option<&'a PathBuf> {
    let file_canon = canonicalize_lossy(file_path);
    let mut best_match: Option<&'a PathBuf> = None;
    let mut best_len = 0;

    for mp in mountpoints {
        let mp_canon = canonicalize_lossy(mp);
        if file_canon.starts_with(&mp_canon) {
            let len = mp_canon.as_os_str().len();
            if len >= best_len {
                best_len = len;
                best_match = Some(mp);
            }
        }
    }

    best_match
}

/// Compute the relative path of `file_path` inside `mountpoint`.
pub fn get_relative_path<P1: AsRef<Path>, P2: AsRef<Path>>(
    file_path: P1,
    mountpoint: P2,
) -> PathBuf {
    let file_canon = canonicalize_lossy(file_path);
    let mp_canon = canonicalize_lossy(mountpoint);

    if let Ok(rel) = file_canon.strip_prefix(&mp_canon) {
        rel.to_path_buf()
    } else {
        file_canon
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path(Path::new("models/sd/v1-5.safetensors")),
            "models/sd/v1-5.safetensors"
        );
        assert_eq!(
            normalize_path(r"C:\mount\models\test.pt"),
            "C:/mount/models/test.pt"
        );
    }

    #[test]
    fn test_matching_mountpoint() {
        let dir = tempdir().unwrap();
        // Resolve any short paths (like RUNNER~1) to long paths on Windows
        let base_path = canonicalize_lossy(dir.path());
        let mp1 = base_path.join("mnt").join("drive1");
        let mp2 = base_path.join("mnt").join("drive1_usb");
        let mp3 = base_path.join("mnt").join("drive2");

        fs::create_dir_all(&mp1).unwrap();
        fs::create_dir_all(&mp2).unwrap();
        fs::create_dir_all(&mp3).unwrap();

        let mountpoints = vec![mp1.clone(), mp2.clone(), mp3.clone()];

        let file1 = mp1.join("models").join("sd.safetensors");
        let matched = find_matching_mountpoint(&file1, &mountpoints);
        assert_eq!(matched, Some(&mp1));

        let file2 = mp2.join("models").join("lora.safetensors");
        let matched2 = find_matching_mountpoint(&file2, &mountpoints);
        assert_eq!(matched2, Some(&mp2));

        let rel = get_relative_path(&file1, &mp1);
        assert_eq!(normalize_path(rel), "models/sd.safetensors");
    }
}
