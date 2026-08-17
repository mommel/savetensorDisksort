//! Hierarchical folder tree with recursive size and file count aggregation.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::discovery::FileInfo;
use crate::utils::format_bytes;

/// Node in the hierarchical folder tree representing a directory with aggregated sizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderNode {
    pub size_bytes: u64,
    pub size_human: String,
    pub file_count: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub children: BTreeMap<String, FolderNode>,
}

impl FolderNode {
    pub fn new() -> Self {
        Self {
            size_bytes: 0,
            size_human: "0 B".to_string(),
            file_count: 0,
            children: BTreeMap::new(),
        }
    }

    /// Insert a file by path components and add to leaf file count/size.
    pub fn insert_file(&mut self, components: &[&str], size_bytes: u64) {
        if components.is_empty() {
            self.size_bytes += size_bytes;
            self.file_count += 1;
            return;
        }

        let first = components[0];
        let child = self
            .children
            .entry(first.to_string())
            .or_insert_with(FolderNode::new);
        child.insert_file(&components[1..], size_bytes);
    }

    /// Recursively calculate and format total sizes and counts bottom-up.
    pub fn aggregate(&mut self) {
        let mut total_size = self.size_bytes;
        let mut total_files = self.file_count;

        for child in self.children.values_mut() {
            child.aggregate();
            total_size += child.size_bytes;
            total_files += child.file_count;
        }

        self.size_bytes = total_size;
        self.file_count = total_files;
        self.size_human = format_bytes(total_size);
    }
}

impl Default for FolderNode {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the root-level folder tree map from a list of discovered files.
pub fn build_folder_tree(files: &[FileInfo]) -> BTreeMap<String, FolderNode> {
    let mut tree: BTreeMap<String, FolderNode> = BTreeMap::new();

    for file in files {
        let root_key = &file.mountpoint;
        let root_node = tree.entry(root_key.clone()).or_insert_with(FolderNode::new);

        let parts: Vec<&str> = file
            .relative_path
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();

        // If there are parent directory components before the filename:
        if parts.len() > 1 {
            let dir_parts = &parts[..parts.len() - 1];
            root_node.insert_file(dir_parts, file.size_bytes);
        } else {
            root_node.size_bytes += file.size_bytes;
            root_node.file_count += 1;
        }
    }

    // Bottom-up aggregation for all mountpoint roots
    for node in tree.values_mut() {
        node.aggregate();
    }

    tree
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::Category;

    #[test]
    fn test_aggregate_sizes() {
        let files = vec![
            FileInfo {
                id: "f-0001".into(),
                real_path: "/mnt/nvme0/models/sd/v1-5.safetensors".into(),
                filename: "v1-5.safetensors".into(),
                extension: "safetensors".into(),
                category: Category::Checkpoint,
                size_bytes: 4_000_000_000,
                size_human: "3.73 GB".into(),
                blake3_hash: None,
                modified_at: None,
                mountpoint: "/mnt/nvme0".into(),
                relative_path: "models/sd/v1-5.safetensors".into(),
                symlinked_from: vec![],
            },
            FileInfo {
                id: "f-0002".into(),
                real_path: "/mnt/nvme0/models/lora/detail.safetensors".into(),
                filename: "detail.safetensors".into(),
                extension: "safetensors".into(),
                category: Category::Lora,
                size_bytes: 100_000_000,
                size_human: "95.4 MB".into(),
                blake3_hash: None,
                modified_at: None,
                mountpoint: "/mnt/nvme0".into(),
                relative_path: "models/lora/detail.safetensors".into(),
                symlinked_from: vec![],
            },
        ];

        let tree = build_folder_tree(&files);
        assert!(tree.contains_key("/mnt/nvme0"));

        let root = &tree["/mnt/nvme0"];
        assert_eq!(root.size_bytes, 4_100_000_000);
        assert_eq!(root.file_count, 2);

        let models = &root.children["models"];
        assert_eq!(models.size_bytes, 4_100_000_000);
        assert_eq!(models.file_count, 2);

        let sd = &models.children["sd"];
        assert_eq!(sd.size_bytes, 4_000_000_000);
        assert_eq!(sd.file_count, 1);

        let lora = &models.children["lora"];
        assert_eq!(lora.size_bytes, 100_000_000);
        assert_eq!(lora.file_count, 1);
    }
}
