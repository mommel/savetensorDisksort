//! BLAKE3 hashing helpers with memory-mapping and buffered streaming fallbacks.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

const BUFFER_SIZE: usize = 8 * 1024 * 1024; // 8 MB buffer

/// Compute the BLAKE3 hash of a file at `path`.
///
/// Uses `memmap2` for files > 1 MB on supported systems, with a fallback
/// to 8 MB buffered streaming if memory-mapping fails or file is empty.
pub fn hash_file<P: AsRef<Path>>(path: P) -> std::io::Result<String> {
    hash_file_with_progress(path, |_| {})
}

/// Compute the BLAKE3 hash of a file with an optional progress callback.
/// The callback receives the number of bytes processed so far.
pub fn hash_file_with_progress<P: AsRef<Path>, F: FnMut(u64)>(
    path: P,
    mut progress_cb: F,
) -> std::io::Result<String> {
    let path = path.as_ref();
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_len = metadata.len();

    // If file is empty, return hash of empty string directly
    if file_len == 0 {
        progress_cb(0);
        return Ok(blake3::hash(b"").to_hex().to_string());
    }

    // Try memory-mapping for non-empty files (best performance for multi-GB files)
    if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
        let hash = blake3::Hasher::new().update_rayon(&mmap).finalize();
        progress_cb(file_len);
        return Ok(hash.to_hex().to_string());
    }

    // Fallback: Buffered streaming
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut processed: u64 = 0;

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        processed += n as u64;
        progress_cb(processed);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Compute BLAKE3 hash of an arbitrary reader.
pub fn hash_reader<R: Read>(mut reader: R) -> std::io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Verify that the file at `path` matches `expected_hash` (case-insensitive hex).
pub fn verify_file_hash<P: AsRef<Path>>(path: P, expected_hash: &str) -> std::io::Result<bool> {
    let actual_hash = hash_file(path)?;
    Ok(actual_hash.eq_ignore_ascii_case(expected_hash.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hash_empty_file() {
        let file = NamedTempFile::new().unwrap();
        let hash = hash_file(file.path()).unwrap();
        let expected = blake3::hash(b"").to_hex().to_string();
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_hash_known_content() {
        let mut file = NamedTempFile::new().unwrap();
        let data = b"SaveTensor DiskSort Test Content 123456";
        file.write_all(data).unwrap();
        file.flush().unwrap();

        let hash = hash_file(file.path()).unwrap();
        let expected = blake3::hash(data).to_hex().to_string();
        assert_eq!(hash, expected);
        assert!(verify_file_hash(file.path(), &expected).unwrap());
        assert!(!verify_file_hash(file.path(), "0000000000000000").unwrap());
    }

    #[test]
    fn test_hash_with_progress() {
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![42u8; 1024 * 1024]; // 1MB
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        let mut max_progress = 0u64;
        let hash = hash_file_with_progress(file.path(), |bytes| {
            if bytes > max_progress {
                max_progress = bytes;
            }
        })
        .unwrap();

        let expected = blake3::hash(&data).to_hex().to_string();
        assert_eq!(hash, expected);
        assert_eq!(max_progress, data.len() as u64);
    }
}
