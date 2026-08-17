pub mod cleanup;
pub mod copy;
pub mod rollback;
pub mod verify;

pub use cleanup::{create_symlink, delete_source_file, update_symlinks};
pub use copy::copy_with_progress;
pub use rollback::{analyze_operation_recovery, execute_recovery_action, RecoveryAction};
pub use verify::{verify_copy, VerificationError};

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::persistence::log::ExecutionLogger;
use crate::planner::{OpStatus, PlanAction, SortPlan};

/// Execution options and callbacks.
pub struct ExecutionOptions<F>
where
    F: FnMut(&str, u64, u64),
{
    pub dry_run: bool,
    pub logger: Option<ExecutionLogger>,
    pub progress_cb: F,
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

/// Execute a complete SortPlan safely.
pub fn execute_plan<F>(
    plan: &mut SortPlan,
    mut options: ExecutionOptions<F>,
) -> Result<Vec<String>, String>
where
    F: FnMut(&str, u64, u64),
{
    let mut dry_run_messages = Vec::new();

    for op in plan.operations.iter_mut() {
        if !op.selected || op.action == PlanAction::Skip {
            op.status = OpStatus::Skipped;
            continue;
        }

        // Check for cancellation
        if let Some(cancel) = &options.cancel_flag {
            if cancel.load(Ordering::Relaxed) {
                op.status = OpStatus::Skipped;
                op.error_message = Some("Execution cancelled by user".into());
                continue;
            }
        }

        op.status = OpStatus::InProgress;
        let start_time = Instant::now();

        if options.dry_run {
            let msg = format!(
                "[DRY-RUN] Would move '{}' -> '{}' ({}). Would update {} symlinks.",
                op.source,
                op.destination,
                op.size_human,
                op.symlinks_to_update.len()
            );
            dry_run_messages.push(msg);
            op.status = OpStatus::Completed;
            continue;
        }

        // Real Execution
        let src_p = Path::new(&op.source);
        let dst_p = Path::new(&op.destination);

        if let Some(logger) = &mut options.logger {
            logger.log_copy_start(&op.op_id, &op.source, &op.destination);
        }

        // 1. Copy
        let op_id_copy = op.op_id.clone();
        let copy_res = copy_with_progress(src_p, dst_p, |copied, total| {
            (options.progress_cb)(&op_id_copy, copied, total);
        });

        if let Err(e) = copy_res {
            op.status = OpStatus::Failed;
            op.error_message = Some(format!("Copy error: {}", e));
            log::error!("Failed copy for {}: {}", op.op_id, e);
            continue;
        }

        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        if let Some(logger) = &mut options.logger {
            logger.log_copy_done(&op.op_id, op.size_bytes, elapsed_ms);
            logger.log_verify_start(&op.op_id);
        }

        // 2. Verify
        let verify_res = verify_copy(src_p, dst_p, |_, _| {});
        let hash = match verify_res {
            Ok(h) => {
                if let Some(logger) = &mut options.logger {
                    logger.log_verify_ok(&op.op_id, &h);
                }
                h
            }
            Err(e) => {
                op.status = OpStatus::Failed;
                op.error_message = Some(format!("Verification error: {}", e));
                log::error!("Verification failed for {}: {}", op.op_id, e);
                // Incomplete or corrupted destination, do not touch source!
                continue;
            }
        };

        // 3. Delete original (only reached if verify succeeded)
        if let Some(logger) = &mut options.logger {
            logger.log_delete_original(&op.op_id);
        }
        if let Err(e) = delete_source_file(src_p) {
            op.status = OpStatus::Failed;
            op.error_message = Some(format!("Delete original error: {}", e));
            log::error!("Failed to delete source {}: {}", op.source, e);
            continue;
        }

        // 4. Update symlinks
        for link in &op.symlinks_to_update {
            if let Some(logger) = &mut options.logger {
                logger.log_symlink_update(&op.op_id, link, &op.destination);
            }
        }
        update_symlinks(src_p, dst_p, &op.symlinks_to_update);

        // 5. Complete
        if let Some(logger) = &mut options.logger {
            logger.log_complete(&op.op_id, Some(&hash));
        }
        op.status = OpStatus::Completed;
    }

    if options.dry_run {
        plan.dry_run_log = Some(dry_run_messages.clone());
    }

    Ok(dry_run_messages)
}
