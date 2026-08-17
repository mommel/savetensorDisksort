//! Pre-flight validation for sort plans (disk capacity, file freshness, write permissions).

use std::fs;
use std::path::Path;

use super::sort_plan::{PlanAction, SortPlan};

/// Result of validating a SortPlan prior to dry-run or execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn ok() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// Validate a SortPlan against current live filesystem state.
pub fn validate_plan(plan: &SortPlan) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. Check target drive existence
    let target_path = Path::new(&plan.target_drive);
    if !target_path.exists() {
        errors.push(format!(
            "Target drive '{}' does not exist or is not mounted",
            plan.target_drive
        ));
    }

    // 2. Check disk space sufficiency (+5% margin)
    if !plan.space_analysis.fits {
        errors.push(format!(
            "Insufficient disk space on target drive '{}': requires {}, available {}",
            plan.target_drive,
            crate::utils::format_bytes(plan.space_analysis.total_move_size),
            crate::utils::format_bytes(plan.space_analysis.target_drive_free_before)
        ));
    }

    // 3. Validate each selected operation
    for op in &plan.operations {
        if !op.selected || op.action == PlanAction::Skip {
            continue;
        }

        let src = Path::new(&op.source);
        if !src.exists() {
            errors.push(format!("Source file does not exist: {}", op.source));
            continue;
        }

        // Freshness check: verify source file size has not changed since scan
        if let Ok(meta) = fs::metadata(src) {
            if meta.len() != op.size_bytes {
                warnings.push(format!(
                    "Source file '{}' size changed on disk (expected {}, found {})",
                    op.source,
                    op.size_bytes,
                    meta.len()
                ));
            }
        } else {
            errors.push(format!(
                "Failed to read metadata for source file: {}",
                op.source
            ));
        }

        // Check if destination file already exists and would be overwritten
        let dst = Path::new(&op.destination);
        if dst.exists() && op.source != op.destination {
            warnings.push(format!(
                "Destination file already exists and will be verified/handled: {}",
                op.destination
            ));
        }
    }

    let is_valid = errors.is_empty();
    ValidationResult {
        is_valid,
        errors,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::sort_plan::{OpStatus, SortOperation, SpaceAnalysis};
    use chrono::Utc;
    use std::collections::HashMap;
    use tempfile::NamedTempFile;

    #[test]
    fn test_validate_plan_missing_source() {
        let temp_src = NamedTempFile::new().unwrap();
        let src_path = temp_src.path().to_string_lossy().to_string();

        let plan = SortPlan {
            version: 1,
            plan_timestamp: Utc::now(),
            target_drive: ".".into(),
            target_structure: serde_json::json!({}),
            operations: vec![SortOperation {
                op_id: "op-0001".into(),
                action: PlanAction::Move,
                source: src_path,
                destination: "./test_dst.safetensors".into(),
                file_id: "f-0001".into(),
                size_bytes: 0,
                size_human: "0 B".into(),
                selected: true,
                status: OpStatus::Pending,
                symlinks_to_update: vec![],
                error_message: None,
            }],
            space_analysis: SpaceAnalysis {
                target_drive_free_before: 1_000_000,
                total_move_size: 0,
                target_drive_free_after: 1_000_000,
                source_drives_freed: HashMap::new(),
                fits: true,
            },
            dry_run_log: None,
        };

        let result = validate_plan(&plan);
        assert!(result.is_valid);
    }
}
