# SaveTensor DiskSort — Developer Guide

## 1. Project Structure

```
savetensorDisksort/
├── Cargo.toml                  # Workspace dependencies and crate definition
├── src/
│   ├── main.rs                 # CLI entrypoint (clap derive)
│   ├── lib.rs                  # Public library exports
│   ├── config.rs               # JSON configuration parser & default builder
│   ├── utils/
│   │   ├── mod.rs
│   │   ├── hash.rs             # BLAKE3 SIMD + memmap2 / buffered fallback
│   │   ├── human_size.rs       # Base-1024 byte formatting & parsing
│   │   └── path_utils.rs       # Cross-platform path normalization & canonicalization
│   ├── discovery/
│   │   ├── mod.rs
│   │   ├── file_info.rs        # FileInfo, Category, MountpointInfo data models
│   │   ├── filters.rs          # MODEL_EXTENSIONS and category heuristics
│   │   ├── walker.rs           # Rayon-parallel physical directory traversal (no symlinks)
│   │   └── symlink_mapper.rs   # Recursive symlink resolver with cycle detection
│   ├── accounting/
│   │   ├── mod.rs
│   │   ├── size_tree.rs        # Hierarchical FolderNode bottom-up aggregation
│   │   └── duplicates.rs       # Duplicate candidate grouping by filename + exact size
│   ├── planner/
│   │   ├── mod.rs
│   │   ├── sort_plan.rs        # SortPlan, SortOperation, and template generators
│   │   ├── conflict.rs         # Target collision resolution (_2, _3 suffixing)
│   │   └── validator.rs        # Space capacity (+5% margin) & freshness checks
│   ├── executor/
│   │   ├── mod.rs
│   │   ├── copy.rs             # 8MB buffered stream copy with progress callback
│   │   ├── verify.rs           # Post-copy BLAKE3 verification
│   │   ├── cleanup.rs          # Source deletion & atomic symlink swap
│   │   └── rollback.rs         # Journal-based crash recovery analyzer
│   ├── persistence/
│   │   ├── mod.rs
│   │   ├── inventory_json.rs   # inventory.json build & serialization
│   │   ├── plan_json.rs        # sort_plan.json serialization
│   │   └── log.rs              # execution.jsonl logger & log reader
│   └── tui/
│       ├── mod.rs
│       ├── app.rs              # Ratatui state machine and event polling loop
│       ├── keybinds.rs         # Keyboard shortcut mapping
│       ├── views/              # Scan, Inventory, Plan, Execute, Summary views
│       └── widgets/            # Tree, CheckboxList, Progress bar widgets
├── tests/
│   ├── integration_tests.rs    # Multi-module filesystem & recovery tests
│   └── e2e_tests.rs            # End-to-end simulated workflow tests
└── docs/                       # Architecture, User, Developer, LLM guides
```

---

## 2. Key APIs & Reference

### 2.1 Discovery Engine
```rust
use savetensor_disksort::discovery::{scan_all_mountpoints, SymlinkMapper, inspect_mountpoint};

let mut files = scan_all_mountpoints(&mountpoint_paths);
let mut mapper = SymlinkMapper::new();
mapper.scan_app_roots(&app_root_paths);
mapper.cross_reference_inventory(&mut files);
```

### 2.2 Sort Planning
```rust
use savetensor_disksort::planner::{SortPlan, PlanTemplate, validate_plan};

let plan = SortPlan::generate(&files, "/mnt/target", free_bytes, PlanTemplate::ByType);
let validation = validate_plan(&plan);
assert!(validation.is_valid);
```

### 2.3 Safe Execution Pipeline
```rust
use savetensor_disksort::executor::{execute_plan, ExecutionOptions};
use savetensor_disksort::persistence::ExecutionLogger;

let logger = ExecutionLogger::new("disksort_data/execution.jsonl")?;
let options = ExecutionOptions {
    dry_run: false,
    logger: Some(logger),
    cancel_flag: None,
    progress_cb: |op_id, copied, total| {
        println!("Progress {}: {} / {}", op_id, copied, total);
    },
};

execute_plan(&mut plan, options)?;
```

---

## 3. Running Tests & Builds

```bash
# Run unit tests
cargo test --lib

# Run all tests (unit, integration, e2e)
cargo test --all

# Validate build without warnings
cargo check --all-targets

# Compile optimized release binary
cargo build --release
```
