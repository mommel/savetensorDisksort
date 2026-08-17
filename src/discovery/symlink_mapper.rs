//! Logical application tree walker and symlink resolver with cycle detection.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::file_info::FileInfo;
use crate::utils::{canonicalize_lossy, is_symlink, normalize_path};

/// Representation of a symlink mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymlinkEntry {
    pub link_path: String,
    pub target_path: String,
    pub is_dir: bool,
    pub is_dangling: bool,
}

/// Recursive node in the symlink tree structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymlinkTreeNode {
    #[serde(rename = "_type", skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(rename = "_target", skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub children: HashMap<String, SymlinkTreeNode>,
}

impl Default for SymlinkTreeNode {
    fn default() -> Self {
        Self::new()
    }
}

impl SymlinkTreeNode {
    pub fn new() -> Self {
        Self {
            node_type: None,
            target: None,
            children: HashMap::new(),
        }
    }
}

/// Symlink tree walker that collects symlinks within application roots.
#[derive(Debug, Default)]
pub struct SymlinkMapper {
    pub mappings: Vec<SymlinkEntry>,
    pub symlink_tree: HashMap<String, SymlinkTreeNode>,
}

impl SymlinkMapper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Walk application roots and discover all symbolic links (files and directories).
    pub fn scan_app_roots(&mut self, app_roots: &[PathBuf]) {
        for root in app_roots {
            if !root.exists() {
                log::warn!("App root '{}' does not exist, skipping", root.display());
                continue;
            }

            let mut visited_paths = HashSet::new();
            let mut tree_root = SymlinkTreeNode::new();
            self.walk_recursive(root, root, &mut visited_paths, &mut tree_root, 0);

            let root_key = normalize_path(canonicalize_lossy(root));
            if !tree_root.children.is_empty() || tree_root.target.is_some() {
                self.symlink_tree.insert(root_key, tree_root);
            }
        }
    }

    fn walk_recursive(
        &mut self,
        current: &Path,
        _app_root: &Path,
        visited: &mut HashSet<PathBuf>,
        node: &mut SymlinkTreeNode,
        depth: usize,
    ) {
        if depth > 64 {
            log::warn!("Max recursion depth exceeded at {}", current.display());
            return;
        }

        let entries = match fs::read_dir(current) {
            Ok(rd) => rd,
            Err(_) => return,
        };

        for entry_res in entries {
            let entry = match entry_res {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            let name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };

            if name == ".git" || name == "node_modules" {
                continue;
            }

            if is_symlink(&path) {
                let target_raw = match fs::read_link(&path) {
                    Ok(t) => t,
                    Err(e) => {
                        log::warn!("Failed to readlink {}: {}", path.display(), e);
                        continue;
                    }
                };

                let target_resolved = if target_raw.is_absolute() {
                    target_raw.clone()
                } else if let Some(parent) = path.parent() {
                    parent.join(&target_raw)
                } else {
                    target_raw.clone()
                };

                let canon_target = canonicalize_lossy(&target_resolved);
                let is_dir = canon_target.is_dir();
                let is_dangling = !canon_target.exists();

                let link_str = normalize_path(canonicalize_lossy(&path));
                let target_str = normalize_path(&canon_target);

                self.mappings.push(SymlinkEntry {
                    link_path: link_str,
                    target_path: target_str.clone(),
                    is_dir,
                    is_dangling,
                });

                let mut child_node = SymlinkTreeNode::new();
                child_node.node_type = Some(if is_dir {
                    "symlink_dir".to_string()
                } else {
                    "symlink_file".to_string()
                });
                child_node.target = Some(target_str);

                // Cycle detection for directory symlinks
                if is_dir && !is_dangling {
                    if !visited.insert(canon_target.clone()) {
                        log::warn!("Symlink cycle detected at {}", path.display());
                    } else {
                        self.walk_recursive(
                            &canon_target,
                            _app_root,
                            visited,
                            &mut child_node,
                            depth + 1,
                        );
                        visited.remove(&canon_target);
                    }
                }

                node.children.insert(name, child_node);
            } else if path.is_dir() {
                let canon_path = canonicalize_lossy(&path);
                if visited.insert(canon_path.clone()) {
                    let mut child_node = SymlinkTreeNode::new();
                    self.walk_recursive(&path, _app_root, visited, &mut child_node, depth + 1);
                    if !child_node.children.is_empty() {
                        node.children.insert(name, child_node);
                    }
                    visited.remove(&canon_path);
                }
            }
        }
    }

    /// Cross-reference discovered symlinks with physical inventory files.
    /// Updates each `FileInfo.symlinked_from` list with symlinks that point to it.
    pub fn cross_reference_inventory(&self, files: &mut [FileInfo]) {
        let mut target_to_links: HashMap<String, Vec<String>> = HashMap::new();

        for entry in &self.mappings {
            target_to_links
                .entry(entry.target_path.clone())
                .or_default()
                .push(entry.link_path.clone());
        }

        for file in files.iter_mut() {
            let canon_real = normalize_path(canonicalize_lossy(&file.real_path));
            if let Some(links) = target_to_links.get(&canon_real) {
                for link in links {
                    if !file.symlinked_from.contains(link) {
                        file.symlinked_from.push(link.clone());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[cfg(windows)]
    use std::os::windows::fs::symlink_file;

    #[test]
    fn test_resolve_symlink_and_cross_ref() {
        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("storage").join("models");
        let app_dir = dir.path().join("app").join("checkpoints");

        fs::create_dir_all(&target_dir).unwrap();
        fs::create_dir_all(&app_dir).unwrap();

        let model_file = target_dir.join("v1-5.safetensors");
        fs::write(&model_file, b"test model content").unwrap();

        let symlink_path = app_dir.join("v1-5.safetensors");

        #[cfg(unix)]
        let link_res = symlink(&model_file, &symlink_path);
        #[cfg(windows)]
        let link_res = symlink_file(&model_file, &symlink_path);

        if link_res.is_ok() {
            let mut mapper = SymlinkMapper::new();
            mapper.scan_app_roots(&[dir.path().join("app")]);

            assert!(!mapper.mappings.is_empty());
            assert_eq!(
                normalize_path(canonicalize_lossy(&model_file)),
                mapper.mappings[0].target_path
            );

            let mut files = vec![FileInfo {
                id: "f-0001".to_string(),
                real_path: normalize_path(canonicalize_lossy(&model_file)),
                filename: "v1-5.safetensors".to_string(),
                extension: "safetensors".to_string(),
                category: crate::discovery::Category::Checkpoint,
                size_bytes: 18,
                size_human: "18 B".to_string(),
                blake3_hash: None,
                modified_at: None,
                mountpoint: normalize_path(dir.path()),
                relative_path: "storage/models/v1-5.safetensors".to_string(),
                symlinked_from: Vec::new(),
            }];

            mapper.cross_reference_inventory(&mut files);
            assert_eq!(files[0].symlinked_from.len(), 1);
        }
    }
}
