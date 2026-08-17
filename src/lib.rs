//! SaveTensor DiskSort Core Library
//!
//! Multi-drive AI model file inventory, planning, and safe relocation engine.

pub mod accounting;
pub mod config;
pub mod discovery;
pub mod executor;
pub mod persistence;
pub mod planner;
pub mod tui;
pub mod utils;

pub use config::DiskSortConfig;
pub use discovery::{Category, FileInfo, MountpointInfo, SymlinkMapper};
pub use executor::{execute_plan, ExecutionOptions};
pub use persistence::{Inventory, InventorySummary, LogEvent, ExecutionLogger};
pub use planner::{PlanAction, PlanTemplate, SortOperation, SortPlan};
pub use utils::format_bytes;
