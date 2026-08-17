//! BLAKE3 checksum verification comparing source and destination files.

use std::path::Path;
use thiserror::Error;

use crate::utils::hash::hash_file_with_progress;

#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("I/O error during checksum calculation: {0}")]
    Io(#[from] std::io::Error),
    #[error("Checksum mismatch: source hash '{source_hash}' != destination hash '{dest_hash}'")]
    HashMismatch {
        source_hash: String,
        dest_hash: String,
    },
}

/// Verify that the destination file has the exact same BLAKE3 hash as the source file.
///
/// Returns `Ok(hash)` when verification succeeds, or `Err(VerificationError)` on mismatch or I/O error.
pub fn verify_copy<P1: AsRef<Path>, P2: AsRef<Path>, F: FnMut(&str, u64)>(
    src: P1,
    dst: P2,
    mut progress_cb: F,
) -> Result<String, VerificationError> {
    let src_hash = hash_file_with_progress(src.as_ref(), |b| progress_cb("source", b))?;
    let dst_hash = hash_file_with_progress(dst.as_ref(), |b| progress_cb("destination", b))?;

    if src_hash.eq_ignore_ascii_case(&dst_hash) {
        Ok(src_hash)
    } else {
        Err(VerificationError::HashMismatch {
            source_hash: src_hash,
            dest_hash: dst_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_verify_copy_success() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src.bin");
        let dst = dir.path().join("dst.bin");

        fs::write(&src, b"Identical model contents 9999").unwrap();
        fs::write(&dst, b"Identical model contents 9999").unwrap();

        let res = verify_copy(&src, &dst, |_, _| {});
        assert!(res.is_ok());
    }

    #[test]
    fn test_verify_copy_mismatch() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src.bin");
        let dst = dir.path().join("dst.bin");

        fs::write(&src, b"Original file content").unwrap();
        fs::write(&dst, b"Corrupted destination content").unwrap();

        let res = verify_copy(&src, &dst, |_, _| {});
        match res {
            Err(VerificationError::HashMismatch { .. }) => {}
            _ => panic!("Expected HashMismatch error"),
        }
    }
}
