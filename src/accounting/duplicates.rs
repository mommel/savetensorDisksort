//! Duplicate file candidate detection by filename, exact size, and BLAKE3 hash.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::discovery::FileInfo;
use crate::utils::format_bytes;

/// Group of potential or confirmed duplicate files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub filename: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub file_ids: Vec<String>,
    pub paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_hash: Option<String>,
}

/// Find duplicate candidates where both filename and exact size match across different paths.
pub fn find_duplicate_candidates(files: &[FileInfo]) -> Vec<DuplicateGroup> {
    let mut candidate_map: HashMap<(&str, u64), Vec<&FileInfo>> = HashMap::new();

    for file in files {
        candidate_map
            .entry((&file.filename, file.size_bytes))
            .or_default()
            .push(file);
    }

    let mut groups = Vec::new();

    for ((filename, size_bytes), matching_files) in candidate_map {
        if matching_files.len() > 1 {
            groups.push(DuplicateGroup {
                filename: filename.to_string(),
                size_bytes,
                size_human: format_bytes(size_bytes),
                file_ids: matching_files.iter().map(|f| f.id.clone()).collect(),
                paths: matching_files.iter().map(|f| f.real_path.clone()).collect(),
                confirmed_hash: None,
            });
        }
    }

    // Sort by largest size first for easier review
    groups.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::Category;

    #[test]
    fn test_find_duplicates() {
        let files = vec![
            FileInfo {
                id: "f-0001".into(),
                real_path: "/mnt/nvme0/models/v1-5.safetensors".into(),
                filename: "v1-5.safetensors".into(),
                extension: "safetensors".into(),
                category: Category::Checkpoint,
                size_bytes: 4_000_000_000,
                size_human: "3.73 GB".into(),
                blake3_hash: None,
                modified_at: None,
                mountpoint: "/mnt/nvme0".into(),
                relative_path: "models/v1-5.safetensors".into(),
                symlinked_from: vec![],
            },
            FileInfo {
                id: "f-0002".into(),
                real_path: "/mnt/usbc1/backup/v1-5.safetensors".into(),
                filename: "v1-5.safetensors".into(),
                extension: "safetensors".into(),
                category: Category::Checkpoint,
                size_bytes: 4_000_000_000,
                size_human: "3.73 GB".into(),
                blake3_hash: None,
                modified_at: None,
                mountpoint: "/mnt/usbc1".into(),
                relative_path: "backup/v1-5.safetensors".into(),
                symlinked_from: vec![],
            },
            FileInfo {
                id: "f-0003".into(),
                real_path: "/mnt/nvme0/lora/detail.safetensors".into(),
                filename: "detail.safetensors".into(),
                extension: "safetensors".into(),
                category: Category::Lora,
                size_bytes: 50_000_000,
                size_human: "47.7 MB".into(),
                blake3_hash: None,
                modified_at: None,
                mountpoint: "/mnt/nvme0".into(),
                relative_path: "lora/detail.safetensors".into(),
                symlinked_from: vec![],
            },
            FileInfo {
                id: "f-0004".into(),
                real_path: "/mnt/usbc1/diff_size/v1-5.safetensors".into(),
                filename: "v1-5.safetensors".into(),
                extension: "safetensors".into(),
                category: Category::Checkpoint,
                size_bytes: 2_000_000_000, // Different size!
                size_human: "1.86 GB".into(),
                blake3_hash: None,
                modified_at: None,
                mountpoint: "/mnt/usbc1".into(),
                relative_path: "diff_size/v1-5.safetensors".into(),
                symlinked_from: vec![],
            },
        ];

        let dupes = find_duplicate_candidates(&files);
        assert_eq!(dupes.len(), 1);
        assert_eq!(dupes[0].filename, "v1-5.safetensors");
        assert_eq!(dupes[0].file_ids.len(), 2);
        assert_eq!(dupes[0].file_ids, vec!["f-0001", "f-0002"]);
    }
}
