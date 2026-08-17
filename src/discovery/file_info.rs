//! Data models for discovered model files, categories, and mountpoints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported model file categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Checkpoint,
    Lora,
    Embedding,
    Vae,
    Controlnet,
    Upscaler,
    Other,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Checkpoint => "checkpoint",
            Category::Lora => "lora",
            Category::Embedding => "embedding",
            Category::Vae => "vae",
            Category::Controlnet => "controlnet",
            Category::Upscaler => "upscaler",
            Category::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "checkpoint" | "checkpoints" | "model" | "models" => Some(Category::Checkpoint),
            "lora" | "loras" => Some(Category::Lora),
            "embedding" | "embeddings" | "textual_inversion" => Some(Category::Embedding),
            "vae" => Some(Category::Vae),
            "controlnet" => Some(Category::Controlnet),
            "upscaler" | "upscalers" | "esrgan" => Some(Category::Upscaler),
            "other" | "misc" => Some(Category::Other),
            _ => None,
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Metadata for a mounted storage drive or directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountpointInfo {
    pub path: String,
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub filesystem: String,
}

/// Discovered real model file with physical location and metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    pub id: String,
    pub real_path: String,
    pub filename: String,
    pub extension: String,
    pub category: Category,
    pub size_bytes: u64,
    pub size_human: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blake3_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<DateTime<Utc>>,
    pub mountpoint: String,
    pub relative_path: String,
    #[serde(default)]
    pub symlinked_from: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_serde() {
        let cat = Category::Checkpoint;
        let serialized = serde_json::to_string(&cat).unwrap();
        assert_eq!(serialized, "\"checkpoint\"");

        let deserialized: Category = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, Category::Checkpoint);
    }

    #[test]
    fn test_category_parsing() {
        assert_eq!(Category::parse("Lora"), Some(Category::Lora));
        assert_eq!(Category::parse("checkpoints"), Some(Category::Checkpoint));
        assert_eq!(
            Category::parse("textual_inversion"),
            Some(Category::Embedding)
        );
        assert_eq!(Category::parse("unknown_xyz"), None);
    }
}
