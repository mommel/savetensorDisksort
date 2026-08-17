//! Sort plan data structures, destination path templates, and plan generation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::discovery::{Category, FileInfo};
use crate::utils::normalize_path;

/// Action to perform for a file during plan execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Move,
    Copy,
    Skip,
    DeleteDuplicate,
}

/// Execution status of an individual operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

/// A single planned relocation or copy operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortOperation {
    pub op_id: String,
    pub action: PlanAction,
    pub source: String,
    pub destination: String,
    pub file_id: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub selected: bool,
    pub status: OpStatus,
    #[serde(default)]
    pub symlinks_to_update: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Space usage and capacity validation results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceAnalysis {
    pub target_drive_free_before: u64,
    pub total_move_size: u64,
    pub target_drive_free_after: u64,
    pub source_drives_freed: HashMap<String, u64>,
    pub fits: bool,
}

/// Supported destination directory templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanTemplate {
    ByType,
    ByTypeAndBase,
    Flat,
    PreserveStructure,
}

impl PlanTemplate {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "by-type" | "by_type" | "type" => Some(PlanTemplate::ByType),
            "by-type-and-base" | "by_type_and_base" | "base" => Some(PlanTemplate::ByTypeAndBase),
            "flat" => Some(PlanTemplate::Flat),
            "preserve" | "preserve-structure" | "relative" => Some(PlanTemplate::PreserveStructure),
            _ => None,
        }
    }
}

/// Complete serializable sort plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortPlan {
    pub version: u32,
    pub plan_timestamp: DateTime<Utc>,
    pub target_drive: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub target_structure: serde_json::Value,
    pub operations: Vec<SortOperation>,
    pub space_analysis: SpaceAnalysis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run_log: Option<Vec<String>>,
}

impl SortPlan {
    /// Generate a new SortPlan from discovered files and target configuration.
    pub fn generate(
        files: &[FileInfo],
        target_drive: &str,
        target_free_bytes: u64,
        template: PlanTemplate,
    ) -> Self {
        let target_norm = normalize_path(target_drive);
        let mut operations = Vec::new();
        let mut dest_counts: HashMap<String, usize> = HashMap::new();

        for (i, file) in files.iter().enumerate() {
            let rel_dest = compute_destination_path(file, template);
            let raw_dest = Path::new(&target_norm).join(&rel_dest);
            let mut dest_str = normalize_path(raw_dest);

            // Check and resolve collisions
            let count = dest_counts.entry(dest_str.clone()).or_insert(0);
            if *count > 0 {
                let path_buf = PathBuf::from(&dest_str);
                let stem = path_buf.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
                let ext = path_buf.extension().and_then(|e| e.to_str()).unwrap_or("");
                let parent = path_buf.parent().unwrap_or_else(|| Path::new(""));

                let new_filename = if ext.is_empty() {
                    format!("{}_{}", stem, *count + 1)
                } else {
                    format!("{}_{}.{}", stem, *count + 1, ext)
                };
                dest_str = normalize_path(parent.join(new_filename));
            }
            *count += 1;

            // If file is already at destination path, mark as skip
            let is_already_in_place = file.real_path == dest_str;
            let action = if is_already_in_place {
                PlanAction::Skip
            } else {
                PlanAction::Move
            };

            operations.push(SortOperation {
                op_id: format!("op-{:04}", i + 1),
                action,
                source: file.real_path.clone(),
                destination: dest_str,
                file_id: file.id.clone(),
                size_bytes: file.size_bytes,
                size_human: file.size_human.clone(),
                selected: !is_already_in_place,
                status: if is_already_in_place {
                    OpStatus::Skipped
                } else {
                    OpStatus::Pending
                },
                symlinks_to_update: file.symlinked_from.clone(),
                error_message: None,
            });
        }

        let space_analysis = calculate_space_analysis(&operations, target_free_bytes);

        Self {
            version: 1,
            plan_timestamp: Utc::now(),
            target_drive: target_norm,
            target_structure: serde_json::json!({
                "models": {
                    "checkpoints": {},
                    "loras": {},
                    "embeddings": {},
                    "vae": {},
                    "controlnet": {},
                    "upscaler": {},
                    "other": {}
                }
            }),
            operations,
            space_analysis,
            dry_run_log: None,
        }
    }
}

/// Compute destination relative subpath based on template and category.
pub fn compute_destination_path(file: &FileInfo, template: PlanTemplate) -> PathBuf {
    match template {
        PlanTemplate::ByType => {
            let folder = match file.category {
                Category::Checkpoint => "models/checkpoints",
                Category::Lora => "models/loras",
                Category::Embedding => "models/embeddings",
                Category::Vae => "models/vae",
                Category::Controlnet => "models/controlnet",
                Category::Upscaler => "models/upscaler",
                Category::Other => "models/other",
            };
            Path::new(folder).join(&file.filename)
        }
        PlanTemplate::ByTypeAndBase => {
            let base = detect_base_model(&file.filename, &file.relative_path);
            let folder = match file.category {
                Category::Checkpoint => format!("models/checkpoints/{}", base),
                Category::Lora => format!("models/loras/{}", base),
                Category::Embedding => format!("models/embeddings/{}", base),
                Category::Vae => "models/vae".to_string(),
                Category::Controlnet => "models/controlnet".to_string(),
                Category::Upscaler => "models/upscaler".to_string(),
                Category::Other => "models/other".to_string(),
            };
            Path::new(&folder).join(&file.filename)
        }
        PlanTemplate::Flat => Path::new("models").join(&file.filename),
        PlanTemplate::PreserveStructure => Path::new(&file.relative_path).to_path_buf(),
    }
}

/// Infer base model architecture (sd15, sdxl, flux, sd3, pony, other) from path/filename.
pub fn detect_base_model(filename: &str, relative_path: &str) -> &'static str {
    let lower = format!("{} {}", filename, relative_path).to_lowercase();
    if lower.contains("flux") {
        "flux"
    } else if lower.contains("pony") {
        "pony"
    } else if lower.contains("sdxl") || lower.contains("xl_") || lower.contains("-xl") {
        "sdxl"
    } else if lower.contains("sd3") || lower.contains("sd_3") {
        "sd3"
    } else if lower.contains("sd15")
        || lower.contains("sd1.5")
        || lower.contains("v1-5")
        || lower.contains("v1.5")
        || lower.contains("1.5")
    {
        "sd15"
    } else {
        "general"
    }
}

/// Calculate free space before and after plan execution.
pub fn calculate_space_analysis(
    operations: &[SortOperation],
    target_free_before: u64,
) -> SpaceAnalysis {
    let mut total_move_size = 0u64;
    let mut source_drives_freed = HashMap::new();

    for op in operations {
        if op.selected && op.action == PlanAction::Move {
            total_move_size += op.size_bytes;

            // Extract mountpoint from source
            let src_path = PathBuf::from(&op.source);
            let mut drive_root = String::new();
            for comp in src_path.components() {
                drive_root.push_str(&comp.as_os_str().to_string_lossy());
                drive_root.push('/');
                break;
            }
            *source_drives_freed.entry(drive_root).or_insert(0u64) += op.size_bytes;
        }
    }

    // Include 5% safety margin check
    let margin_limit = (target_free_before as f64 * 0.95) as u64;
    let fits = total_move_size <= margin_limit;
    let target_free_after = target_free_before.saturating_sub(total_move_size);

    SpaceAnalysis {
        target_drive_free_before: target_free_before,
        total_move_size,
        target_drive_free_after: target_free_after,
        source_drives_freed,
        fits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_plan_basic() {
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
                symlinked_from: vec!["/home/user/ComfyUI/models/checkpoints/v1-5.safetensors".into()],
            },
            FileInfo {
                id: "f-0002".into(),
                real_path: "/mnt/nvme0/models/detail.safetensors".into(),
                filename: "detail.safetensors".into(),
                extension: "safetensors".into(),
                category: Category::Lora,
                size_bytes: 100_000_000,
                size_human: "95.4 MB".into(),
                blake3_hash: None,
                modified_at: None,
                mountpoint: "/mnt/nvme0".into(),
                relative_path: "models/detail.safetensors".into(),
                symlinked_from: vec![],
            },
        ];

        let plan = SortPlan::generate(&files, "/mnt/usbc1", 500_000_000_000, PlanTemplate::ByType);

        assert_eq!(plan.operations.len(), 2);
        assert_eq!(plan.operations[0].op_id, "op-0001");
        assert_eq!(plan.operations[0].action, PlanAction::Move);
        assert_eq!(
            plan.operations[0].destination,
            "/mnt/usbc1/models/checkpoints/v1-5.safetensors"
        );
        assert_eq!(
            plan.operations[1].destination,
            "/mnt/usbc1/models/loras/detail.safetensors"
        );
        assert!(plan.space_analysis.fits);
    }
}
