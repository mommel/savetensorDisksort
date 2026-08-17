//! Configuration loader and default generator (`disksort.json`).

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Individual configured mountpoint entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigMountpoint {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Global tool configuration loaded from `disksort.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskSortConfig {
    pub mountpoints: Vec<ConfigMountpoint>,
    #[serde(default)]
    pub app_roots: Vec<String>,
    #[serde(default = "default_extensions")]
    pub file_extensions: Vec<String>,
    #[serde(default = "default_exclude_patterns")]
    pub exclude_patterns: Vec<String>,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    #[serde(default = "default_hash_algorithm")]
    pub hash_algorithm: String,
    #[serde(default = "default_copy_buffer_size")]
    pub copy_buffer_size_mb: usize,
    #[serde(default = "default_true")]
    pub verify_after_copy: bool,
    #[serde(default = "default_true")]
    pub update_symlinks_after_move: bool,
}

fn default_extensions() -> Vec<String> {
    vec![
        "safetensors".into(),
        "pt".into(),
        "pth".into(),
        "ckpt".into(),
        "bin".into(),
        "gguf".into(),
        "onnx".into(),
    ]
}

fn default_exclude_patterns() -> Vec<String> {
    vec!["*.tmp".into(), "*.partial".into(), ".git/**".into()]
}

fn default_output_dir() -> String {
    "./disksort_data".into()
}

fn default_hash_algorithm() -> String {
    "blake3".into()
}

fn default_copy_buffer_size() -> usize {
    8
}

fn default_true() -> bool {
    true
}

impl Default for DiskSortConfig {
    fn default() -> Self {
        Self {
            mountpoints: Vec::new(),
            app_roots: Vec::new(),
            file_extensions: default_extensions(),
            exclude_patterns: default_exclude_patterns(),
            output_dir: default_output_dir(),
            hash_algorithm: default_hash_algorithm(),
            copy_buffer_size_mb: default_copy_buffer_size(),
            verify_after_copy: true,
            update_symlinks_after_move: true,
        }
    }
}

impl DiskSortConfig {
    /// Load configuration from a file, falling back to default if file is missing.
    pub fn load_from_file_or_default<P: AsRef<Path>>(path: P) -> Self {
        let p = path.as_ref();
        if p.exists() {
            if let Ok(file) = File::open(p) {
                let reader = BufReader::new(file);
                if let Ok(cfg) = serde_json::from_reader(reader) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    /// Save configuration to file.
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

    /// Returns list of mountpoints as `PathBuf`s.
    pub fn mountpoint_paths(&self) -> Vec<PathBuf> {
        self.mountpoints
            .iter()
            .map(|mp| PathBuf::from(&mp.path))
            .collect()
    }

    /// Returns list of app roots as `PathBuf`s.
    pub fn app_root_paths(&self) -> Vec<PathBuf> {
        self.app_roots.iter().map(PathBuf::from).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_defaults_and_serde() {
        let cfg = DiskSortConfig::default();
        let file = NamedTempFile::new().unwrap();

        cfg.save_to_file(file.path()).unwrap();
        let loaded = DiskSortConfig::load_from_file_or_default(file.path());

        assert_eq!(cfg.file_extensions, loaded.file_extensions);
        assert_eq!(cfg.output_dir, loaded.output_dir);
    }
}
