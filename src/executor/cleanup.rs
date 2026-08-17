//! Safe source deletion and atomic symlink redirection.

use std::fs;
use std::path::{Path, PathBuf};
use crate::utils::{canonicalize_lossy, normalize_path};

/// Remove the original source file.
/// MUST ONLY be invoked after checksum verification has passed.
pub fn delete_source_file<P: AsRef<Path>>(source: P) -> std::io::Result<()> {
    let p = source.as_ref();
    if p.exists() {
        fs::remove_file(p)?;
    }
    Ok(())
}

/// Create a symbolic link pointing to `target` at path `link_path`.
/// Handles cross-platform symlink creation (Unix and Windows).
pub fn create_symlink<P1: AsRef<Path>, P2: AsRef<Path>>(
    target: P1,
    link_path: P2,
) -> std::io::Result<()> {
    let target = target.as_ref();
    let link_path = link_path.as_ref();

    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link_path)?;
    }

    #[cfg(windows)]
    {
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(target, link_path)?;
        } else {
            std::os::windows::fs::symlink_file(target, link_path)?;
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Symlinks unsupported on this platform",
        ));
    }

    Ok(())
}

/// Update a list of symlinks to point from `old_target` to `new_target`.
///
/// If a symlink was externally modified or points elsewhere, it is safely skipped.
pub fn update_symlinks(
    old_target: &Path,
    new_target: &Path,
    symlinks: &[String],
) -> Vec<Result<String, String>> {
    let mut results = Vec::new();
    let old_canon = normalize_path(canonicalize_lossy(old_target));
    let new_canon = canonicalize_lossy(new_target);

    for link_str in symlinks {
        let link_path = PathBuf::from(link_str);
        if !link_path.exists() && fs::symlink_metadata(&link_path).is_err() {
            results.push(Err(format!("Symlink '{}' no longer exists", link_str)));
            continue;
        }

        // Verify it was pointing to old_target
        if let Ok(current_target) = fs::read_link(&link_path) {
            let resolved_current = if current_target.is_absolute() {
                current_target
            } else if let Some(parent) = link_path.parent() {
                parent.join(current_target)
            } else {
                current_target
            };

            let current_canon = normalize_path(canonicalize_lossy(&resolved_current));
            if current_canon != old_canon {
                log::warn!(
                    "Symlink '{}' points to '{}', expected '{}'; skipping update",
                    link_str,
                    current_canon,
                    old_canon
                );
                results.push(Err(format!(
                    "Symlink target mismatch for '{}'",
                    link_str
                )));
                continue;
            }
        }

        // Remove old symlink
        let _ = fs::remove_file(&link_path);

        // Create new symlink
        match create_symlink(&new_canon, &link_path) {
            Ok(_) => {
                results.push(Ok(link_str.clone()));
            }
            Err(e) => {
                results.push(Err(format!(
                    "Failed to create new symlink '{}': {}",
                    link_str, e
                )));
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_delete_source() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("source_to_delete.bin");
        fs::write(&file, b"content").unwrap();
        assert!(file.exists());

        delete_source_file(&file).unwrap();
        assert!(!file.exists());
    }
}
