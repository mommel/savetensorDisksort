//! Crash recovery and operation journal analysis.

use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::executor::cleanup::{delete_source_file, update_symlinks};
use crate::executor::verify::verify_copy;
use crate::persistence::log::LogEvent;

/// Recovery status of an interrupted operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryAction {
    CleanIncompleteDest { dst: String },
    Reverify { src: String, dst: String },
    ResumeSymlinkUpdate { src: String, dst: String, symlinks: Vec<String> },
    AlreadyCompleted,
    None,
}

/// Analyze log events for a specific operation ID to determine recovery step.
pub fn analyze_operation_recovery(events: &[LogEvent]) -> RecoveryAction {
    let mut has_copy_start = false;
    let mut has_copy_done = false;
    let mut has_verify_ok = false;
    let mut has_delete_original = false;
    let mut has_complete = false;

    let mut src = String::new();
    let mut dst = String::new();
    let mut symlinks = Vec::new();

    for ev in events {
        match ev.phase.as_str() {
            "copy_start" => {
                has_copy_start = true;
                if let Some(s) = &ev.src {
                    src = s.clone();
                }
                if let Some(d) = &ev.dst {
                    dst = d.clone();
                }
            }
            "copy_done" => has_copy_done = true,
            "verify_ok" => has_verify_ok = true,
            "delete_original" => has_delete_original = true,
            "symlink_update" => {
                if let Some(p) = &ev.path {
                    symlinks.push(p.clone());
                }
            }
            "complete" => has_complete = true,
            _ => {}
        }
    }

    if has_complete {
        return RecoveryAction::AlreadyCompleted;
    }

    if has_delete_original {
        return RecoveryAction::ResumeSymlinkUpdate {
            src,
            dst,
            symlinks,
        };
    }

    if has_verify_ok {
        return RecoveryAction::Reverify { src, dst };
    }

    if has_copy_done {
        return RecoveryAction::Reverify { src, dst };
    }

    if has_copy_start {
        return RecoveryAction::CleanIncompleteDest { dst };
    }

    RecoveryAction::None
}

/// Execute crash recovery for an interrupted operation based on analysis.
pub fn execute_recovery_action(action: &RecoveryAction) -> Result<String, String> {
    match action {
        RecoveryAction::CleanIncompleteDest { dst } => {
            let p = Path::new(dst);
            if p.exists() {
                fs::remove_file(p).map_err(|e| e.to_string())?;
            }
            let tmp = p.with_extension(format!(
                "{}.disksort_tmp",
                p.extension().and_then(|e| e.to_str()).unwrap_or("")
            ));
            if tmp.exists() {
                fs::remove_file(tmp).map_err(|e| e.to_string())?;
            }
            Ok(format!("Cleaned incomplete destination '{}'", dst))
        }
        RecoveryAction::Reverify { src, dst } => {
            let src_p = Path::new(src);
            let dst_p = Path::new(dst);
            if !src_p.exists() {
                return Err(format!("Source file '{}' missing during reverify", src));
            }
            if !dst_p.exists() {
                return Err(format!("Destination file '{}' missing during reverify", dst));
            }

            match verify_copy(src_p, dst_p, |_, _| {}) {
                Ok(hash) => {
                    delete_source_file(src_p).map_err(|e| e.to_string())?;
                    Ok(format!(
                        "Re-verified hash '{}', deleted source '{}'",
                        hash, src
                    ))
                }
                Err(e) => Err(format!("Re-verification failed: {}", e)),
            }
        }
        RecoveryAction::ResumeSymlinkUpdate {
            src,
            dst,
            symlinks,
        } => {
            let src_p = Path::new(src);
            let dst_p = Path::new(dst);
            update_symlinks(src_p, dst_p, symlinks);
            Ok(format!("Updated {} symlinks to '{}'", symlinks.len(), dst))
        }
        RecoveryAction::AlreadyCompleted | RecoveryAction::None => Ok("No action needed".into()),
    }
}
