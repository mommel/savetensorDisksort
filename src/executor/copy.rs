//! Buffered file copy engine with progress reporting and data synchronization.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

const BUFFER_SIZE: usize = 8 * 1024 * 1024; // 8 MB buffer

/// Copy a file from `src` to `dst` using 8MB buffered streams with live progress updates.
///
/// Pre-creates parent directories and executes `sync_all()` upon completion to ensure
/// all bytes are committed to physical disk.
pub fn copy_with_progress<P1: AsRef<Path>, P2: AsRef<Path>, F: FnMut(u64, u64)>(
    src: P1,
    dst: P2,
    mut progress_cb: F,
) -> std::io::Result<u64> {
    let src_path = src.as_ref();
    let dst_path = dst.as_ref();

    if let Some(parent) = dst_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let src_file = File::open(src_path)?;
    let total_bytes = src_file.metadata()?.len();

    // Use temporary destination during copy to avoid incomplete files in final path
    let tmp_dst_path = dst_path.with_extension(format!(
        "{}.disksort_tmp",
        dst_path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));

    let dst_file = File::create(&tmp_dst_path)?;

    let mut reader = BufReader::with_capacity(BUFFER_SIZE, src_file);
    let mut writer = BufWriter::with_capacity(BUFFER_SIZE, dst_file);
    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut copied: u64 = 0;

    progress_cb(0, total_bytes);

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buffer[..n])?;
        copied += n as u64;
        progress_cb(copied, total_bytes);
    }

    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);

    // Atomically rename temporary file to destination path
    if dst_path.exists() {
        fs::remove_file(dst_path)?;
    }
    fs::rename(&tmp_dst_path, dst_path)?;

    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_copy_creates_dirs_and_preserves_content() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source.dat");
        let dst = dir.path().join("nested").join("sub").join("dest.dat");

        let test_data = b"Hello SaveTensor DiskSort Buffered Copy Engine!";
        fs::write(&src, test_data).unwrap();

        let mut last_copied = 0u64;
        let mut reported_total = 0u64;

        let copied = copy_with_progress(&src, &dst, |c, t| {
            last_copied = c;
            reported_total = t;
        })
        .unwrap();

        assert_eq!(copied, test_data.len() as u64);
        assert_eq!(last_copied, test_data.len() as u64);
        assert_eq!(reported_total, test_data.len() as u64);

        let read_back = fs::read(&dst).unwrap();
        assert_eq!(read_back, test_data);
    }
}
