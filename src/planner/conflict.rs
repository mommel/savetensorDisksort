//! Destination naming conflict resolution and deduplication logic.

use std::path::{Path, PathBuf};
use crate::utils::normalize_path;

/// Generate a suffixed filename (e.g. `model_2.safetensors`) if destination collision occurs.
pub fn generate_unique_destination<P: AsRef<Path>>(
    dest_path: P,
    existing_destinations: &[String],
) -> String {
    let original = normalize_path(dest_path.as_ref());
    if !existing_destinations.contains(&original) {
        return original;
    }

    let path_buf = PathBuf::from(&original);
    let stem = path_buf
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = path_buf.extension().and_then(|e| e.to_str()).unwrap_or("");
    let parent = path_buf.parent().unwrap_or_else(|| Path::new(""));

    let mut counter = 2usize;
    loop {
        let new_filename = if ext.is_empty() {
            format!("{}_{}", stem, counter)
        } else {
            format!("{}_{}.{}", stem, counter, ext)
        };
        let candidate = normalize_path(parent.join(new_filename));
        if !existing_destinations.contains(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_destination() {
        let existing = vec![
            "/mnt/usbc1/models/checkpoints/v1-5.safetensors".to_string(),
            "/mnt/usbc1/models/checkpoints/v1-5_2.safetensors".to_string(),
        ];

        let target = "/mnt/usbc1/models/checkpoints/v1-5.safetensors";
        let unique = generate_unique_destination(target, &existing);
        assert_eq!(unique, "/mnt/usbc1/models/checkpoints/v1-5_3.safetensors");
    }
}
