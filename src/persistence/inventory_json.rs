//! Serialization and summary aggregation for `inventory.json`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::Path;

use crate::accounting::{build_folder_tree, find_duplicate_candidates, FolderNode};
use crate::discovery::{FileInfo, MountpointInfo, SymlinkTreeNode};
use crate::utils::format_bytes;

/// Breakdown statistics for an individual model category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategorySummary {
    pub count: usize,
    pub size_human: String,
}

/// Breakdown statistics for an individual storage mountpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountpointSummary {
    pub count: usize,
    pub size_human: String,
}

/// Aggregated high-level inventory statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySummary {
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub total_size_human: String,
    pub by_category: HashMap<String, CategorySummary>,
    pub by_mountpoint: HashMap<String, MountpointSummary>,
    pub duplicate_candidates: usize,
}

/// Complete multi-drive inventory state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub version: u32,
    pub scan_timestamp: DateTime<Utc>,
    pub mountpoints: Vec<MountpointInfo>,
    pub files: Vec<FileInfo>,
    pub folder_tree: BTreeMap<String, FolderNode>,
    pub symlink_tree: HashMap<String, SymlinkTreeNode>,
    pub summary: InventorySummary,
}

impl Inventory {
    /// Build complete Inventory structure from scanned files, mountpoints, and symlinks.
    pub fn build(
        mountpoints: Vec<MountpointInfo>,
        files: Vec<FileInfo>,
        symlink_tree: HashMap<String, SymlinkTreeNode>,
    ) -> Self {
        let folder_tree = build_folder_tree(&files);
        let duplicates = find_duplicate_candidates(&files);

        let mut total_size_bytes = 0u64;
        let mut cat_counts: HashMap<String, (usize, u64)> = HashMap::new();
        let mut mp_counts: HashMap<String, (usize, u64)> = HashMap::new();

        for f in &files {
            total_size_bytes += f.size_bytes;

            let cat_key = f.category.as_str().to_string();
            let cat_entry = cat_counts.entry(cat_key).or_insert((0, 0));
            cat_entry.0 += 1;
            cat_entry.1 += f.size_bytes;

            let mp_key = f.mountpoint.clone();
            let mp_entry = mp_counts.entry(mp_key).or_insert((0, 0));
            mp_entry.0 += 1;
            mp_entry.1 += f.size_bytes;
        }

        let mut by_category = HashMap::new();
        for (cat, (count, size)) in cat_counts {
            by_category.insert(
                cat,
                CategorySummary {
                    count,
                    size_human: format_bytes(size),
                },
            );
        }

        let mut by_mountpoint = HashMap::new();
        for (mp, (count, size)) in mp_counts {
            by_mountpoint.insert(
                mp,
                MountpointSummary {
                    count,
                    size_human: format_bytes(size),
                },
            );
        }

        let summary = InventorySummary {
            total_files: files.len(),
            total_size_bytes,
            total_size_human: format_bytes(total_size_bytes),
            by_category,
            by_mountpoint,
            duplicate_candidates: duplicates.len(),
        };

        Self {
            version: 1,
            scan_timestamp: Utc::now(),
            mountpoints,
            files,
            folder_tree,
            symlink_tree,
            summary,
        }
    }

    /// Save the inventory to disk as formatted JSON.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(p)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }

    /// Load an existing inventory from disk.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let inventory = serde_json::from_reader(reader)?;
        Ok(inventory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_inventory_save_load() {
        let inv = Inventory::build(vec![], vec![], HashMap::new());
        let file = NamedTempFile::new().unwrap();

        inv.save_to_file(file.path()).unwrap();
        let loaded = Inventory::load_from_file(file.path()).unwrap();

        assert_eq!(inv.version, loaded.version);
        assert_eq!(inv.summary.total_files, loaded.summary.total_files);
    }
}
