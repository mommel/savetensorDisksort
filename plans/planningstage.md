# SaveTensor DiskSort — Development Plan

> **Project**: Multi-drive AI model file inventory, planning & relocation tool  
> **Target OS**: Linux (ext4/btrfs/xfs on internal + USB-C drives)  
> **Language**: Rust (rationale in §2)  
> **Status**: Planning Stage  
> **Created**: 2026-08-17  

---

## Table of Contents

1. [Problem Statement](#1-problem-statement)
2. [Technology Decision — Why Rust](#2-technology-decision--why-rust)
3. [Architecture Overview](#3-architecture-overview)
4. [Module Breakdown](#4-module-breakdown)
5. [Data Model & File Formats](#5-data-model--file-formats)
6. [Phase 1 — Discovery Engine](#6-phase-1--discovery-engine)
7. [Phase 2 — Symlink Tree Walker](#7-phase-2--symlink-tree-walker)
8. [Phase 3 — Size Accounting](#8-phase-3--size-accounting)
9. [Phase 4 — Sort Planner](#9-phase-4--sort-planner)
10. [Phase 5 — TUI Interface](#10-phase-5--tui-interface)
11. [Phase 6 — Execution Engine (Copy → Verify → Delete)](#11-phase-6--execution-engine-copy--verify--delete)
12. [Phase 7 — Integration & End-to-End Tests](#12-phase-7--integration--end-to-end-tests)
13. [Known Pitfalls & Mitigations](#13-known-pitfalls--mitigations)
14. [TDD Strategy & Testable Steps](#14-tdd-strategy--testable-steps)
15. [CLI Interface Design](#15-cli-interface-design)
16. [Milestone Schedule](#16-milestone-schedule)
17. [Appendix A — Example JSON Outputs](#appendix-a--example-json-outputs)
18. [Appendix B — Glossary](#appendix-b--glossary)

---

## 1. Problem Statement

A Linux workstation with 3–4 storage drives (internal NVMe/SATA + USB-C enclosures) hosts a large collection of AI model files:

| Extension | Description |
|---|---|
| `.safetensors` | Stable Diffusion / LLM model weights (SafeTensors format) |
| `.pt` / `.pth` | PyTorch checkpoint files |
| `.bin` | GGUF / ONNX / misc binary model files |
| Embedding folders | Textual-inversion embeddings (often small `.pt` or `.safetensors`) |
| LoRA folders | Low-rank adaptation weights |

These files are scattered across drives and stitched together with `ln -s` symlinks so that consuming tools (ComfyUI, A1111, Forge, etc.) can find them. This works but creates an opaque, fragile structure where:

- It's unclear **which physical drive** actually holds a file.
- Disk space accounting is unreliable (symlinks hide the real location).
- Moving or renaming a drive breaks dozens of symlinks.
- Duplicates may exist across drives without anyone noticing.

### Goal

Build a **single, robust CLI/TUI tool** that:

1. **Inventories** all real (non-symlink) model files across specified mountpoints.
2. **Maps** the symlink tree to understand the logical view applications see.
3. **Reports** precise disk usage per folder/subfolder in GB.
4. **Plans** a consolidation ("sort") — user picks a target drive/structure, tool generates a deterministic plan.
5. **Executes** the plan safely: copy first, checksum-verify, delete original only on match.
6. Provides a **TUI (terminal UI)** for interactive plan review, folder selection, dry-run, and execution.

---

## 2. Technology Decision — Why Rust

| Criterion | Rust | Python | Go |
|---|---|---|---|
| **Filesystem speed** | ★★★★★ — zero-cost abstractions, parallel `walkdir` | ★★☆☆☆ — GIL, `os.walk` is single-threaded | ★★★★☆ — fast, but less control |
| **Large file hashing** | ★★★★★ — `blake3` crate, SIMD-accelerated | ★★★☆☆ — hashlib is C, but Python overhead | ★★★★☆ — decent |
| **TUI ecosystem** | ★★★★★ — `ratatui` is best-in-class | ★★★☆☆ — `textual` is good but heavier | ★★★☆☆ — `bubbletea` is good |
| **Single binary deploy** | ★★★★★ — `cargo build --release`, one file, no runtime | ★☆☆☆☆ — needs venv/pip | ★★★★★ — single binary |
| **Safety (no data loss)** | ★★★★★ — ownership model prevents dangling state | ★★☆☆☆ — runtime errors | ★★★☆☆ — nil panics |
| **Cross-compile for ARM** | ★★★★☆ — `cross` crate | ★★★★☆ — if pure Python | ★★★★★ — trivial |
| **Development speed** | ★★★☆☆ — steeper curve, but strong type safety pays off | ★★★★★ — fastest to prototype | ★★★★☆ — fast |

**Decision**: **Rust**. The tool handles multi-GB files, parallel I/O, checksumming, and must never lose data. Rust's ownership model and zero-cost abstractions make it the safest and fastest choice. The single-binary deployment is a major UX win — no Python environment to manage on a machine already cluttered with AI tool dependencies.

**Fallback**: If Rust development proves too slow for the timeline, the architecture is designed so that a Python rewrite using `asyncio` + `textual` is straightforward. The data model (JSON) is language-agnostic.

---

## 3. Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                        CLI / TUI Frontend                        │
│  (ratatui)  Commands: scan, plan, sort, status                   │
├──────────────────────────────────────────────────────────────────┤
│                        Core Engine                               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐ │
│  │ Discovery│  │ Symlink  │  │  Size    │  │  Sort Planner    │ │
│  │ Engine   │  │ Mapper   │  │ Accountr │  │  (Plan Generator)│ │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └───────┬──────────┘ │
│       │              │             │                │            │
│  ┌────▼──────────────▼─────────────▼────────────────▼──────────┐ │
│  │                   Data Model (in-memory)                    │ │
│  │            DriveInventory / SortPlan structs                 │ │
│  └─────────────────────────┬───────────────────────────────────┘ │
│                            │                                     │
│  ┌─────────────────────────▼───────────────────────────────────┐ │
│  │            Execution Engine                                 │ │
│  │   Copy → BLAKE3 Verify → Symlink Update → Delete Original  │ │
│  └─────────────────────────────────────────────────────────────┘ │
├──────────────────────────────────────────────────────────────────┤
│                    Persistence (JSON / JSONL)                    │
│  inventory.json · symlink_map.json · sort_plan.json · log.jsonl  │
└──────────────────────────────────────────────────────────────────┘
```

### Key Architectural Decisions

1. **Scan and plan are separate from execution.** You can scan today, review the plan tomorrow, execute next week. All state is serialized to JSON.
2. **Execution is always copy-first.** No `mv` across filesystems. Always `cp` → verify → `rm`. This is slower but **guarantees no data loss**.
3. **Checksums use BLAKE3**, not SHA-256. BLAKE3 is ~3–5× faster on modern CPUs, cryptographically secure, and parallelizable. For multi-GB model files this is a significant time saving.
4. **The TUI is optional.** Every operation can be driven purely via CLI flags and JSON files. The TUI is a convenience layer.

---

## 4. Module Breakdown

```
src/
├── main.rs                 # CLI entrypoint (clap)
├── lib.rs                  # Public API re-exports
├── config.rs               # Configuration loading & validation
├── discovery/
│   ├── mod.rs
│   ├── walker.rs           # Physical file walker (no symlinks)
│   ├── symlink_mapper.rs   # Symlink tree resolver
│   ├── file_info.rs        # FileInfo struct + metadata extraction
│   └── filters.rs          # Extension filters (.safetensors, .pt, etc.)
├── accounting/
│   ├── mod.rs
│   ├── size_tree.rs        # Recursive folder size aggregation
│   └── duplicates.rs       # Duplicate detection (by hash / by name)
├── planner/
│   ├── mod.rs
│   ├── sort_plan.rs        # SortPlan struct + generation logic
│   ├── conflict.rs         # Naming conflict resolution
│   └── validator.rs        # Plan validation (disk space, paths)
├── executor/
│   ├── mod.rs
│   ├── copy.rs             # Buffered copy with progress
│   ├── verify.rs           # BLAKE3 checksum comparison
│   ├── cleanup.rs          # Original file deletion + symlink update
│   └── rollback.rs         # Undo partial operations on failure
├── tui/
│   ├── mod.rs
│   ├── app.rs              # ratatui App state machine
│   ├── views/
│   │   ├── scan_view.rs    # Scan progress display
│   │   ├── inventory_view.rs # Browse inventory tree
│   │   ├── plan_view.rs    # Review & toggle plan items
│   │   ├── exec_view.rs    # Execution progress + log
│   │   └── summary_view.rs # Post-execution summary
│   ├── widgets/
│   │   ├── tree.rs         # Collapsible folder tree widget
│   │   ├── checkbox_list.rs# Selectable item list
│   │   └── progress_bar.rs # Transfer progress bar
│   └── keybinds.rs         # Keyboard shortcut definitions
├── persistence/
│   ├── mod.rs
│   ├── inventory_json.rs   # Serialize/deserialize inventory
│   ├── plan_json.rs        # Serialize/deserialize sort plan
│   └── log.rs              # Append-only JSONL execution log
└── utils/
    ├── mod.rs
    ├── hash.rs             # BLAKE3 hashing helpers
    ├── human_size.rs       # Bytes → "4.2 GB" formatting
    └── path_utils.rs       # Canonicalize, resolve, mountpoint detection
```

---

## 5. Data Model & File Formats

### 5.1 Inventory JSON (`inventory.json`)

This is the primary output of the **scan** phase. One file per scan run.

```jsonc
{
  "version": 1,
  "scan_timestamp": "2026-08-17T20:30:00Z",
  "mountpoints": [
    {
      "path": "/mnt/nvme0",
      "label": "NVMe-Main",
      "total_bytes": 2000398934016,
      "free_bytes": 450000000000,
      "filesystem": "ext4"
    },
    {
      "path": "/mnt/usbc1",
      "label": "USB-C-WD4TB",
      "total_bytes": 4000787030016,
      "free_bytes": 1200000000000,
      "filesystem": "btrfs"
    }
  ],
  "files": [
    {
      "id": "f-0001",
      "real_path": "/mnt/nvme0/models/sd/v1-5-pruned.safetensors",
      "filename": "v1-5-pruned.safetensors",
      "extension": "safetensors",
      "category": "checkpoint",
      "size_bytes": 4265380864,
      "size_human": "3.97 GB",
      "blake3_hash": null,
      "modified_at": "2025-03-14T10:22:00Z",
      "mountpoint": "/mnt/nvme0",
      "relative_path": "models/sd/v1-5-pruned.safetensors",
      "symlinked_from": [
        "/home/user/ComfyUI/models/checkpoints/v1-5-pruned.safetensors",
        "/home/user/stable-diffusion-webui/models/Stable-diffusion/v1-5-pruned.safetensors"
      ]
    }
  ],
  "folder_tree": {
    "/mnt/nvme0": {
      "size_bytes": 128849018880,
      "size_human": "120.0 GB",
      "file_count": 34,
      "children": {
        "models": {
          "size_bytes": 128849018880,
          "size_human": "120.0 GB",
          "file_count": 34,
          "children": {
            "sd": { "size_bytes": 85899345920, "size_human": "80.0 GB", "file_count": 20, "children": {} },
            "lora": { "size_bytes": 42949672960, "size_human": "40.0 GB", "file_count": 14, "children": {} }
          }
        }
      }
    }
  },
  "symlink_tree": {
    "/home/user/ComfyUI/models": {
      "checkpoints": {
        "_type": "symlink_dir",
        "_target": "/mnt/nvme0/models/sd",
        "children": {}
      }
    }
  },
  "summary": {
    "total_files": 247,
    "total_size_bytes": 892345678901,
    "total_size_human": "831.0 GB",
    "by_category": {
      "checkpoint": { "count": 45, "size_human": "620.0 GB" },
      "lora": { "count": 180, "size_human": "190.0 GB" },
      "embedding": { "count": 22, "size_human": "21.0 GB" }
    },
    "by_mountpoint": {
      "/mnt/nvme0": { "count": 89, "size_human": "310.0 GB" },
      "/mnt/usbc1": { "count": 158, "size_human": "521.0 GB" }
    },
    "duplicate_candidates": 12
  }
}
```

### 5.2 Sort Plan JSON (`sort_plan.json`)

Generated by the **plan** phase. Editable by the user (or via TUI).

```jsonc
{
  "version": 1,
  "plan_timestamp": "2026-08-17T21:00:00Z",
  "target_drive": "/mnt/usbc1",
  "target_structure": {
    "models": {
      "checkpoints": { "sd15": {}, "sdxl": {}, "flux": {} },
      "loras": { "sd15": {}, "sdxl": {}, "flux": {} },
      "embeddings": {},
      "vae": {},
      "controlnet": {}
    }
  },
  "operations": [
    {
      "op_id": "op-0001",
      "action": "move",
      "source": "/mnt/nvme0/models/sd/v1-5-pruned.safetensors",
      "destination": "/mnt/usbc1/models/checkpoints/sd15/v1-5-pruned.safetensors",
      "file_id": "f-0001",
      "size_bytes": 4265380864,
      "size_human": "3.97 GB",
      "selected": true,
      "status": "pending",
      "symlinks_to_update": [
        "/home/user/ComfyUI/models/checkpoints/v1-5-pruned.safetensors",
        "/home/user/stable-diffusion-webui/models/Stable-diffusion/v1-5-pruned.safetensors"
      ]
    }
  ],
  "space_analysis": {
    "target_drive_free_before": 1200000000000,
    "total_move_size": 310000000000,
    "target_drive_free_after": 890000000000,
    "source_drives_freed": {
      "/mnt/nvme0": 310000000000
    },
    "fits": true
  },
  "dry_run_log": null
}
```

### 5.3 Execution Log (`execution.jsonl`)

Append-only JSONL log. Every atomic operation gets a line. Enables crash recovery.

```jsonc
{"ts":"2026-08-17T21:15:00Z","op_id":"op-0001","phase":"copy_start","src":"/mnt/nvme0/...","dst":"/mnt/usbc1/..."}
{"ts":"2026-08-17T21:15:42Z","op_id":"op-0001","phase":"copy_done","bytes":4265380864,"elapsed_ms":42000}
{"ts":"2026-08-17T21:15:43Z","op_id":"op-0001","phase":"verify_start"}
{"ts":"2026-08-17T21:16:10Z","op_id":"op-0001","phase":"verify_ok","hash":"a1b2c3d4..."}
{"ts":"2026-08-17T21:16:10Z","op_id":"op-0001","phase":"delete_original"}
{"ts":"2026-08-17T21:16:10Z","op_id":"op-0001","phase":"symlink_update","path":"/home/user/ComfyUI/...","new_target":"/mnt/usbc1/..."}
{"ts":"2026-08-17T21:16:11Z","op_id":"op-0001","phase":"complete"}
```

### 5.4 Why JSON (not TOML, YAML, SQLite)

| Format | Pros | Cons | Verdict |
|---|---|---|---|
| **JSON** | Universal, human-readable, great Rust support (`serde_json`), easy to pipe into `jq` | Verbose, no comments (use `.jsonc` convention) | ✅ **Chosen** |
| TOML | Great for config | Poor fit for deeply nested data / arrays of objects | ❌ Inventory is too nested |
| YAML | Compact | Whitespace-sensitive, security footguns, ambiguous types | ❌ Too fragile for critical data |
| SQLite | Fast queries, ACID | Overkill, not human-inspectable, harder to diff/version | ❌ Not needed at this scale |

JSON files are easy to `cat`, `jq`, diff, and version-control. For a tool managing precious data, inspectability is paramount.

---

## 6. Phase 1 — Discovery Engine

### 6.1 Objective

Walk each user-specified mountpoint and collect every real (non-symlink) file matching the target extensions.

### 6.2 Detailed Design

```
Input:  List of mountpoints (e.g., ["/mnt/nvme0", "/mnt/usbc1", "/mnt/usbc2"])
Output: Vec<FileInfo>  — all discovered model files with metadata

Algorithm:
  for each mountpoint in config:
    assert mountpoint exists and is a directory
    assert mountpoint is actually a mount (check /proc/mounts)
    parallel walkdir(mountpoint):
      for each DirEntry:
        if entry.file_type().is_symlink():
          skip (record in symlink log for Phase 2)
        if entry.file_type().is_file():
          if extension matches FILTER_SET:
            collect FileInfo {
              real_path: entry.path().canonicalize(),
              size: entry.metadata().len(),
              modified: entry.metadata().modified(),
              mountpoint: determine_mountpoint(entry.path()),
              ...
            }
```

### 6.3 Extension Filter Set

```rust
const MODEL_EXTENSIONS: &[&str] = &[
    "safetensors",
    "pt",
    "pth",
    "ckpt",
    "bin",          // catch GGUF, ONNX, misc
    "gguf",
    "onnx",
];

// Embeddings are often .pt or .safetensors in specific folders.
// We detect by path heuristics: parent folder named "embeddings" or "textual_inversion"
```

### 6.4 Parallelism Strategy

Use `rayon` to walk multiple mountpoints concurrently. Within each mountpoint, `walkdir` is single-threaded (filesystem I/O is typically I/O-bound, not CPU-bound), but multiple mountpoints = multiple physical drives = genuine parallelism wins.

### 6.5 Pitfalls

| Pitfall | Mitigation |
|---|---|
| USB drive disconnects mid-scan | Check mountpoint is live before and periodically during walk. Graceful error with partial results saved. |
| Permission denied on some folders | Log warning, skip, continue. Never abort entire scan for one bad folder. |
| Enormous directory trees (millions of files) | Use `ignore` crate (same engine as `ripgrep`) for fast, memory-efficient walking. Filter early to avoid collecting non-model files. |
| Mountpoint is actually a symlink itself | Resolve mountpoint path with `canonicalize()` before walking. |

### 6.6 Testable Steps (TDD)

| # | Test | Description |
|---|---|---|
| T1.1 | `test_filter_extensions` | Unit: given filenames, correctly identify model files |
| T1.2 | `test_skip_symlinks` | Unit: mock filesystem with symlinks, verify they are excluded from file list |
| T1.3 | `test_walk_empty_dir` | Unit: empty mountpoint returns empty Vec, no error |
| T1.4 | `test_walk_nested_structure` | Integration: create temp dir tree with model files, verify all found |
| T1.5 | `test_mountpoint_detection` | Unit: given a path, correctly determine which mountpoint it belongs to |
| T1.6 | `test_permission_denied_skipped` | Integration: create unreadable folder, verify scan continues |

---

## 7. Phase 2 — Symlink Tree Walker

### 7.1 Objective

Walk the *logical* tree (following symlinks) to build the application-visible structure. Map every symlink to its real target.

### 7.2 Detailed Design

```
Input:  List of "application roots" to scan for symlinks
        (e.g., ["/home/user/ComfyUI", "/home/user/stable-diffusion-webui"])
        OR: auto-detect by scanning common locations
Output: SymlinkMap { symlink_path → real_target_path }
        SymlinkTree { tree structure with _type: "symlink_dir" | "symlink_file" annotations }

Algorithm:
  for each app_root:
    walkdir(app_root, follow_links=true):
      for each DirEntry:
        if entry is symlink:
          target = readlink(entry) → canonicalize
          record SymlinkMapping { link: entry.path, target: target }
          annotate in tree
```

### 7.3 Symlink Cycle Detection

> **⚠️ PITFALL**: Symlink cycles (`A → B → C → A`) will cause infinite recursion.

**Mitigation**: Track visited inodes (`device_id, inode_number` pairs). If we revisit an inode, log a warning and stop recursing into that branch. The `walkdir` crate has built-in cycle detection via `follow_links` + `same_file_system`, but we add our own as defense-in-depth.

### 7.4 Cross-referencing with Discovery

After both Phase 1 and Phase 2 complete, we cross-reference:

- For each real file from Phase 1, attach a list of all symlinks that point to it (the `symlinked_from` field in the inventory).
- For each symlink from Phase 2, verify the target exists in the Phase 1 inventory. If not, flag as **dangling** or **external** (pointing to a file not in our scanned mountpoints).

### 7.5 Testable Steps (TDD)

| # | Test | Description |
|---|---|---|
| T2.1 | `test_resolve_simple_symlink` | Unit: symlink → real file correctly mapped |
| T2.2 | `test_resolve_chained_symlinks` | Unit: A → B → C correctly resolves to C |
| T2.3 | `test_detect_symlink_cycle` | Unit: cyclic symlinks detected without hanging |
| T2.4 | `test_dangling_symlink_flagged` | Unit: symlink to nonexistent target is flagged |
| T2.5 | `test_cross_reference_inventory` | Integration: symlinks correctly attached to FileInfo |

---

## 8. Phase 3 — Size Accounting

### 8.1 Objective

Build a hierarchical folder tree with accurate size information at every level.

### 8.2 Detailed Design

```
Input:  Vec<FileInfo> from Phase 1
Output: FolderTree — nested structure with aggregated sizes

Algorithm:
  tree = empty FolderTree
  for each file in inventory:
    path_components = file.relative_path.split('/')
    current_node = tree.root[file.mountpoint]
    for each component in path_components[..last]:
      current_node = current_node.get_or_create_child(component)
    current_node.add_file(file.size_bytes)
  
  // Bottom-up aggregation
  tree.root.aggregate_sizes()  // sums children recursively
```

### 8.3 Size Formatting Rules

- Bytes < 1 KB → show bytes (e.g., "847 B")
- KB range → 1 decimal (e.g., "4.2 KB")
- MB range → 1 decimal (e.g., "128.5 MB")
- GB range → 2 decimals (e.g., "3.97 GB")
- TB range → 2 decimals (e.g., "1.24 TB")
- **Always use base-1024** (GiB displayed as "GB" for user familiarity, but noted in docs)

### 8.4 Duplicate Detection

As a sub-phase, detect potential duplicates:

1. **By filename + size**: Same filename and exact same size on different drives → high probability duplicate.
2. **By hash** (deferred): Only compute BLAKE3 hashes when the user requests verification or during sort execution. Hashing multi-GB files is expensive and should be opt-in.

### 8.5 Testable Steps (TDD)

| # | Test | Description |
|---|---|---|
| T3.1 | `test_aggregate_sizes` | Unit: three files in nested folders, verify parent sizes sum correctly |
| T3.2 | `test_human_size_formatting` | Unit: known byte values → expected human strings |
| T3.3 | `test_duplicate_detection_by_name_size` | Unit: two files with same name+size flagged |
| T3.4 | `test_no_false_duplicate` | Unit: same name but different size NOT flagged |
| T3.5 | `test_empty_folder_zero_size` | Unit: folder with no model files shows 0 B |

---

## 9. Phase 4 — Sort Planner

### 9.1 Objective

Generate a `sort_plan.json` that describes exactly what will be moved where, with space validation.

### 9.2 Inputs

1. The inventory from Phase 1–3.
2. User-specified **target drive** (mountpoint).
3. User-specified **target folder structure** (or use a default template).
4. User **selections** — which folders/files to include in the sort (via TUI checkboxes or JSON editing).

### 9.3 Target Structure Templates

Provide sensible defaults the user can customize:

```
Template: "by-type" (default)
  models/
  ├── checkpoints/
  ├── loras/
  ├── embeddings/
  ├── vae/
  ├── controlnet/
  ├── upscaler/
  └── other/

Template: "by-type-and-base"
  models/
  ├── checkpoints/
  │   ├── sd15/
  │   ├── sdxl/
  │   ├── flux/
  │   └── other/
  ├── loras/
  │   ├── sd15/
  │   ├── sdxl/
  │   ├── flux/
  │   └── other/
  └── ...

Template: "flat"
  models/
  └── (all files in one directory)

Template: "custom"
  (user defines via JSON or TUI)
```

### 9.4 Category Detection Heuristics

| Category | Detection Logic |
|---|---|
| `checkpoint` | `.safetensors` or `.ckpt` in a folder named `checkpoints`, `Stable-diffusion`, or file > 1 GB |
| `lora` | In a folder named `loras`, `Lora`, or file < 500 MB with `.safetensors` |
| `embedding` | In a folder named `embeddings`, `textual_inversion`, or file < 100 MB with `.pt`/`.safetensors` |
| `vae` | Filename contains `vae` (case-insensitive) |
| `controlnet` | Filename/path contains `controlnet`, `control_v11`, `control_` |
| `upscaler` | Filename/path contains `ESRGAN`, `RealESRGAN`, `SwinIR`, `upscale` |
| `other` | Anything not matched above |

> **⚠️ PITFALL**: Heuristics will misclassify some files. The TUI must allow manual override before execution. The plan is always reviewable and editable.

### 9.5 Space Validation

Before finalizing the plan:

```
required_space = sum(op.size_bytes for op in operations where op.selected and op.action == "move")
available_space = target_drive.free_bytes

if required_space > available_space * 0.95:  // 5% safety margin
    ERROR: "Insufficient space on target drive"
    
// Also check: can we fit the largest single file?
largest_file = max(op.size_bytes for op in operations)
if largest_file > available_space:
    ERROR: "Largest file exceeds available space"
```

### 9.6 Conflict Resolution

When two files would end up at the same destination path:

1. **Same hash** → keep one, mark other as `delete_duplicate`.
2. **Different hash, same name** → append suffix: `model.safetensors` → `model_2.safetensors`. Log a warning.
3. **User override** → TUI allows manual rename.

### 9.7 Dry Run

`sort --dry-run` performs all validation, logs what *would* happen, but touches no files. Output is written to `sort_plan.json` with `dry_run_log` populated.

### 9.8 Testable Steps (TDD)

| # | Test | Description |
|---|---|---|
| T4.1 | `test_generate_plan_basic` | Unit: 5 files, target drive, verify plan has 5 operations |
| T4.2 | `test_category_detection` | Unit: known filenames → expected categories |
| T4.3 | `test_space_validation_pass` | Unit: enough space → plan.fits == true |
| T4.4 | `test_space_validation_fail` | Unit: not enough space → error returned |
| T4.5 | `test_conflict_same_name` | Unit: two files same destination → suffix applied |
| T4.6 | `test_dry_run_no_side_effects` | Integration: dry-run produces plan but filesystem unchanged |
| T4.7 | `test_symlink_update_planned` | Unit: moved file's symlinks listed in operation |
| T4.8 | `test_template_by_type` | Unit: verify default template produces correct folder structure |

---

## 10. Phase 5 — TUI Interface

### 10.1 Technology

- **`ratatui`** (Rust) — mature, well-documented, renders to any terminal.
- **`crossterm`** — cross-platform terminal backend.

### 10.2 Screen Layout

```
┌─────────────────────────────────────────────────────────────────────────┐
│  SaveTensor DiskSort v0.1.0                           [Q]uit [?]Help   │
├─────────────────────────────────────────────────────────────────────────┤
│  ◄ Scan │ Inventory │ Plan │ Execute │ Summary ►        Tab navigation │
├─────────────────────────┬───────────────────────────────────────────────┤
│  Folder Tree            │  Details Panel                               │
│                         │                                              │
│  ▼ /mnt/nvme0 (120 GB)  │  Selected: v1-5-pruned.safetensors          │
│    ▼ models (120 GB)    │  Size: 3.97 GB                              │
│      ▶ sd (80 GB)       │  Real path: /mnt/nvme0/models/sd/...        │
│      ▶ lora (40 GB)     │  Symlinked from:                            │
│  ▼ /mnt/usbc1 (521 GB)  │    → /home/user/ComfyUI/models/ckpt/...    │
│    ▶ models (521 GB)    │    → /home/user/webui/models/SD/...         │
│                         │  Category: checkpoint                       │
│                         │  Modified: 2025-03-14                       │
├─────────────────────────┴───────────────────────────────────────────────┤
│  Status: Scan complete — 247 files, 831.0 GB across 3 drives          │
│  [S]can  [P]lan  [D]ry-run  [E]xecute  [R]efresh                     │
└─────────────────────────────────────────────────────────────────────────┘
```

### 10.3 TUI Views

| View | Purpose | Key Interactions |
|---|---|---|
| **Scan** | Run/monitor scan progress | Start scan, see progress bar per mountpoint |
| **Inventory** | Browse discovered files | Tree navigation, expand/collapse, search/filter |
| **Plan** | Configure sort plan | Select target drive, choose template, toggle individual files with checkboxes `[x]`/`[ ]`, set category overrides |
| **Execute** | Run the sort | Start dry-run or real execution, progress bars per file, live log |
| **Summary** | Post-execution report | Files moved, space freed, errors, new symlink map |

### 10.4 Key Bindings

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | Switch between views |
| `↑` / `↓` / `j` / `k` | Navigate tree / list |
| `Enter` / `→` | Expand folder / select item |
| `←` | Collapse folder |
| `Space` | Toggle checkbox (plan view) |
| `a` | Select all / deselect all |
| `/` | Search / filter |
| `d` | Dry-run selected operations |
| `x` | Execute selected operations |
| `q` | Quit (with confirmation if operations pending) |
| `?` | Help overlay |

### 10.5 Testable Steps (TDD)

| # | Test | Description |
|---|---|---|
| T5.1 | `test_tree_widget_render` | Unit: given FolderTree, verify rendered output |
| T5.2 | `test_checkbox_toggle` | Unit: space key toggles selection state |
| T5.3 | `test_view_navigation` | Unit: tab cycles through views correctly |
| T5.4 | `test_search_filter` | Unit: typing filter narrows visible items |
| T5.5 | `test_size_display_alignment` | Unit: size column right-aligned, consistent formatting |

---

## 11. Phase 6 — Execution Engine (Copy → Verify → Delete)

### 11.1 Objective

Safely relocate files according to the approved sort plan. **Zero data loss is the #1 requirement.**

### 11.2 Execution Pipeline (per operation)

```
┌─────────┐    ┌──────────┐    ┌─────────┐    ┌────────────┐    ┌──────────┐
│ 1. Hash │───▶│ 2. Copy  │───▶│ 3. Hash │───▶│ 4. Compare │───▶│ 5. Clean │
│ Source  │    │ to Dest  │    │ Dest    │    │ Hashes     │    │ up       │
└─────────┘    └──────────┘    └─────────┘    └────────────┘    └──────────┘
                                                   │
                                              match?─── No ──▶ ⚠️ ABORT op
                                                │               keep both
                                               Yes              log error
                                                │
                                     ┌──────────▼──────────┐
                                     │ Delete original      │
                                     │ Update symlinks      │
                                     │ Log completion       │
                                     └─────────────────────┘
```

### 11.3 Copy Strategy

```rust
// Buffered copy with progress reporting
const BUFFER_SIZE: usize = 8 * 1024 * 1024; // 8 MB buffer

fn copy_with_progress(src: &Path, dst: &Path, progress: &ProgressSender) -> Result<u64> {
    // 1. Create parent directories
    fs::create_dir_all(dst.parent().unwrap())?;
    
    // 2. Open source and destination
    let src_file = File::open(src)?;
    let dst_file = File::create(dst)?;  // create new, fail if exists
    
    // 3. Pre-allocate destination (fallocate) for performance
    fallocate(&dst_file, src_file.metadata()?.len())?;
    
    // 4. Buffered copy loop
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, src_file);
    let mut writer = BufWriter::with_capacity(BUFFER_SIZE, dst_file);
    let mut copied = 0u64;
    
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 { break; }
        writer.write_all(&buffer[..n])?;
        copied += n as u64;
        progress.send(copied);  // update TUI progress bar
    }
    
    // 5. fsync destination to ensure data on disk
    writer.get_ref().sync_all()?;
    
    Ok(copied)
}
```

### 11.4 Hash Verification

```rust
fn hash_file(path: &Path) -> Result<blake3::Hash> {
    let file = File::open(path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };  // mmap for speed on large files
    Ok(blake3::hash(&mmap))
}
```

> **Why mmap?** For files > 1 GB, memory-mapped I/O avoids double-buffering and lets the OS manage page caching. BLAKE3 can then process the data at near-memcpy speed with SIMD.

> **⚠️ PITFALL**: mmap on a USB drive that disconnects mid-hash will cause a SIGBUS. **Mitigation**: Install a SIGBUS handler that catches this and falls back to buffered reading. Also check that the drive is still mounted before starting.

### 11.5 Symlink Update

After a file moves from `/old/path/model.safetensors` to `/new/path/model.safetensors`:

```rust
fn update_symlinks(old_target: &Path, new_target: &Path, symlinks: &[PathBuf]) -> Result<()> {
    for link in symlinks {
        // 1. Verify symlink still points to old target
        let current_target = fs::read_link(link)?;
        if current_target.canonicalize()? != old_target.canonicalize()? {
            warn!("Symlink {} no longer points to expected target, skipping", link.display());
            continue;
        }
        
        // 2. Remove old symlink
        fs::remove_file(link)?;
        
        // 3. Create new symlink to new location
        std::os::unix::fs::symlink(new_target, link)?;
        
        // 4. Verify new symlink works
        assert!(link.exists(), "Updated symlink is broken!");
    }
    Ok(())
}
```

### 11.6 Crash Recovery

The JSONL execution log enables crash recovery:

1. On startup, check for `execution.jsonl`.
2. If it exists and the last operation is not `"complete"`, we have a partial operation.
3. **Recovery logic**:
   - If `copy_done` but no `verify_ok` → re-verify the destination file.
   - If `verify_ok` but no `delete_original` → both copies exist. Ask user which to keep.
   - If `delete_original` but no `symlink_update` → resume symlink updates.

### 11.7 Testable Steps (TDD)

| # | Test | Description |
|---|---|---|
| T6.1 | `test_copy_creates_dirs` | Integration: copy to nonexistent nested path creates dirs |
| T6.2 | `test_copy_exact_content` | Integration: source and destination byte-identical |
| T6.3 | `test_hash_matches_after_copy` | Integration: BLAKE3 of source == BLAKE3 of destination |
| T6.4 | `test_hash_mismatch_aborts` | Unit: mismatched hashes → error, both files kept |
| T6.5 | `test_original_deleted_after_verify` | Integration: after successful verify, source removed |
| T6.6 | `test_symlink_updated` | Integration: symlink now points to new location |
| T6.7 | `test_symlink_not_updated_if_changed` | Unit: symlink changed by external process → skipped |
| T6.8 | `test_crash_recovery_partial_copy` | Integration: simulate crash after copy, verify recovery |
| T6.9 | `test_execution_log_written` | Integration: every phase logged to JSONL |
| T6.10 | `test_progress_reported` | Unit: progress sender receives incremental updates |

---

## 12. Phase 7 — Integration & End-to-End Tests

### 12.1 Test Infrastructure

Create a `tests/fixtures/` directory with scripts to build fake filesystem trees:

```bash
# tests/fixtures/create_test_env.sh
# Creates:
#   /tmp/disksort_test/
#   ├── drive_a/        (simulated mountpoint)
#   │   └── models/
#   │       ├── sd/
#   │       │   ├── model1.safetensors  (1 MB)
#   │       │   └── model2.safetensors  (2 MB)
#   │       └── lora/
#   │           └── lora1.safetensors   (500 KB)
#   ├── drive_b/        (simulated mountpoint)
#   │   └── models/
#   │       └── model1.safetensors      (1 MB, duplicate)
#   ├── app_root/       (simulated application)
#   │   └── models/
#   │       ├── checkpoints -> /tmp/disksort_test/drive_a/models/sd
#   │       └── loras -> /tmp/disksort_test/drive_a/models/lora
#   └── target_drive/   (empty target)
```

### 12.2 End-to-End Test Scenarios

| # | Scenario | Validates |
|---|---|---|
| E2E-1 | Full scan → plan → dry-run → execute on test fixture | Entire pipeline |
| E2E-2 | Scan with disconnected USB drive | Graceful degradation |
| E2E-3 | Sort with insufficient target space | Error before any file touched |
| E2E-4 | Sort with duplicates across drives | Duplicate detection + resolution |
| E2E-5 | Sort with symlink cycles | Cycle detection doesn't hang |
| E2E-6 | Execute, kill mid-copy, restart | Crash recovery |
| E2E-7 | Sort plan loaded from edited JSON | User-modified plan works |

---

## 13. Known Pitfalls & Mitigations

| # | Pitfall | Severity | Mitigation |
|---|---|---|---|
| P1 | **USB drive disconnects mid-operation** | 🔴 Critical | Check mount status before each operation. Use `inotify` on mountpoint. SIGBUS handler for mmap. |
| P2 | **File modified between hash and copy** | 🟡 Medium | Compare mtime before and after copy. If changed, abort and re-scan. |
| P3 | **Symlink cycles cause infinite recursion** | 🟡 Medium | Track visited inodes. Max recursion depth of 64. |
| P4 | **Destination filesystem doesn't support large files** | 🟡 Medium | Check filesystem type. Warn if FAT32 (4 GB limit). |
| P5 | **Filename encoding issues (non-UTF8)** | 🟢 Low | Use `OsString` throughout. Log unrepresentable names. |
| P6 | **Race condition: another process moves a file** | 🟡 Medium | Lock files during operation (advisory locks via `flock`). |
| P7 | **Out of memory with millions of files** | 🟢 Low | Streaming walker, don't collect all entries in memory at once. Use arena allocator for tree. |
| P8 | **Hardlinks mistaken for duplicates** | 🟢 Low | Compare `(device_id, inode)` pairs. Same inode = same file, not a duplicate. |
| P9 | **Target drive fills up mid-sort** | 🔴 Critical | Check remaining space before each file copy (not just at plan time). Abort gracefully. |
| P10 | **Permissions prevent deletion of source** | 🟡 Medium | Check write permission on source directory before starting. Warn in plan. |
| P11 | **Same file on same filesystem** | 🟢 Low | If source and destination are on the same filesystem, use `rename()` (atomic, instant) instead of copy+delete. |
| P12 | **Interrupted symlink update leaves broken links** | 🟡 Medium | Use atomic symlink swap: create new link with temp name, then `rename()` over old link. |
| P13 | **Plan stale after filesystem changes** | 🟡 Medium | Validate plan freshness: check that all source files still exist and match recorded sizes before execution. |

---

## 14. TDD Strategy & Testable Steps

### 14.1 Test Pyramid

```
         ╱╲
        ╱  ╲       E2E Tests (7 scenarios)
       ╱    ╲      Full pipeline on test fixtures
      ╱──────╲
     ╱        ╲    Integration Tests (~15)
    ╱          ╲   Multi-module interactions, real filesystem
   ╱────────────╲
  ╱              ╲  Unit Tests (~40)
 ╱                ╲ Pure functions, mocked I/O
╱──────────────────╲
```

### 14.2 Test Organization

```
tests/
├── unit/
│   ├── test_filters.rs
│   ├── test_size_formatting.rs
│   ├── test_category_detection.rs
│   ├── test_plan_generation.rs
│   ├── test_conflict_resolution.rs
│   └── test_hash_comparison.rs
├── integration/
│   ├── test_discovery.rs
│   ├── test_symlink_mapping.rs
│   ├── test_copy_verify.rs
│   ├── test_execution_pipeline.rs
│   └── test_crash_recovery.rs
├── e2e/
│   ├── test_full_pipeline.rs
│   └── test_error_scenarios.rs
└── fixtures/
    ├── create_test_env.sh
    └── sample_inventory.json
```

### 14.3 Development Order (TDD Cycle)

Each phase follows: **Red → Green → Refactor**

1. Write failing test for the next smallest unit of functionality.
2. Implement minimum code to pass.
3. Refactor while keeping tests green.
4. Commit.

**Suggested development order:**

```
Phase 1: Discovery
  T1.1 → T1.2 → T1.3 → T1.4 → T1.5 → T1.6
  
Phase 3: Size Accounting (depends on Phase 1 data structures)
  T3.1 → T3.2 → T3.3 → T3.4 → T3.5
  
Phase 2: Symlink Mapper (can be developed in parallel with Phase 3)
  T2.1 → T2.2 → T2.3 → T2.4 → T2.5
  
Phase 4: Sort Planner (depends on Phase 1+2+3)
  T4.1 → T4.2 → T4.3 → T4.4 → T4.5 → T4.6 → T4.7 → T4.8
  
Phase 6: Execution Engine (depends on Phase 4)
  T6.1 → T6.2 → T6.3 → T6.4 → T6.5 → T6.6 → T6.7 → T6.8 → T6.9 → T6.10
  
Phase 5: TUI (depends on all above for data, but UI can be stubbed)
  T5.1 → T5.2 → T5.3 → T5.4 → T5.5
  
Phase 7: E2E
  E2E-1 → E2E-2 → ... → E2E-7
```

---

## 15. CLI Interface Design

### 15.1 Command Structure

```bash
# Top-level
disksort <COMMAND> [OPTIONS]

# Commands
disksort scan       # Discover files across mountpoints
disksort inventory  # View/query the last scan results
disksort plan       # Generate or load a sort plan
disksort sort       # Execute the sort plan
disksort tui        # Launch interactive terminal UI
disksort doctor     # Health check: find broken symlinks, orphaned files

# Global options
--config <PATH>     # Config file (default: ./disksort.json)
--output-dir <DIR>  # Where to write JSON files (default: ./disksort_data/)
--verbose           # Increase log verbosity
--quiet             # Suppress non-error output
```

### 15.2 Example Workflows

```bash
# 1. Quick scan of two drives
disksort scan --mountpoints /mnt/nvme0,/mnt/usbc1 --app-roots /home/user/ComfyUI

# 2. View inventory
disksort inventory --summary
disksort inventory --duplicates
disksort inventory --by-drive

# 3. Generate sort plan
disksort plan --target /mnt/usbc1 --template by-type-and-base

# 4. Dry run
disksort sort --dry-run

# 5. Execute (with confirmation prompt)
disksort sort --execute

# 6. Interactive mode (recommended for first-time users)
disksort tui
```

### 15.3 Configuration File (`disksort.json`)

```jsonc
{
  "mountpoints": [
    { "path": "/mnt/nvme0", "label": "NVMe-Main" },
    { "path": "/mnt/usbc1", "label": "USB-C-WD4TB" },
    { "path": "/mnt/usbc2", "label": "USB-C-Samsung2TB" }
  ],
  "app_roots": [
    "/home/user/ComfyUI",
    "/home/user/stable-diffusion-webui",
    "/home/user/kohya_ss"
  ],
  "file_extensions": ["safetensors", "pt", "pth", "ckpt", "bin", "gguf", "onnx"],
  "exclude_patterns": ["*.tmp", "*.partial", ".git/**"],
  "output_dir": "./disksort_data",
  "hash_algorithm": "blake3",
  "copy_buffer_size_mb": 8,
  "verify_after_copy": true,
  "update_symlinks_after_move": true
}
```

---

## 16. Milestone Schedule

| Milestone | Phases | Key Deliverable | Est. Effort |
|---|---|---|---|
| **M1: Core Discovery** | P1 + P3 | `disksort scan` works, produces `inventory.json` with sizes | 2–3 days |
| **M2: Symlink Mapping** | P2 | Symlink tree resolved, cross-referenced with inventory | 1–2 days |
| **M3: Sort Planner** | P4 | `disksort plan` generates `sort_plan.json`, dry-run works | 2–3 days |
| **M4: Execution Engine** | P6 | `disksort sort --execute` safely moves files | 3–4 days |
| **M5: TUI** | P5 | Full interactive terminal UI | 3–4 days |
| **M6: Polish & E2E** | P7 | Crash recovery, edge cases, E2E tests pass | 2–3 days |
| **Total** | | | **~14–19 days** |

> Milestones are sequential but P2 and P3 can be developed in parallel. P5 (TUI) can begin skeleton work as early as M1.

---

## Appendix A — Example JSON Outputs

### A.1 Minimal Inventory

```json
{
  "version": 1,
  "scan_timestamp": "2026-08-17T20:30:00Z",
  "mountpoints": [
    {
      "path": "/mnt/nvme0",
      "label": "NVMe-Main",
      "total_bytes": 500107862016,
      "free_bytes": 120000000000,
      "filesystem": "ext4"
    }
  ],
  "files": [
    {
      "id": "f-0001",
      "real_path": "/mnt/nvme0/ai/models/sd15/v1-5-pruned-emaonly.safetensors",
      "filename": "v1-5-pruned-emaonly.safetensors",
      "extension": "safetensors",
      "category": "checkpoint",
      "size_bytes": 2132803584,
      "size_human": "1.99 GB",
      "blake3_hash": null,
      "modified_at": "2024-11-20T08:00:00Z",
      "mountpoint": "/mnt/nvme0",
      "relative_path": "ai/models/sd15/v1-5-pruned-emaonly.safetensors",
      "symlinked_from": []
    }
  ],
  "folder_tree": {
    "/mnt/nvme0": {
      "size_bytes": 2132803584,
      "size_human": "1.99 GB",
      "file_count": 1,
      "children": {
        "ai": {
          "size_bytes": 2132803584,
          "size_human": "1.99 GB",
          "file_count": 1,
          "children": {
            "models": {
              "size_bytes": 2132803584,
              "size_human": "1.99 GB",
              "file_count": 1,
              "children": {
                "sd15": {
                  "size_bytes": 2132803584,
                  "size_human": "1.99 GB",
                  "file_count": 1,
                  "children": {}
                }
              }
            }
          }
        }
      }
    }
  },
  "symlink_tree": {},
  "summary": {
    "total_files": 1,
    "total_size_bytes": 2132803584,
    "total_size_human": "1.99 GB",
    "by_category": {
      "checkpoint": { "count": 1, "size_human": "1.99 GB" }
    },
    "by_mountpoint": {
      "/mnt/nvme0": { "count": 1, "size_human": "1.99 GB" }
    },
    "duplicate_candidates": 0
  }
}
```

### A.2 Minimal Sort Plan

```json
{
  "version": 1,
  "plan_timestamp": "2026-08-17T21:00:00Z",
  "target_drive": "/mnt/usbc1",
  "target_structure": {
    "models": {
      "checkpoints": {}
    }
  },
  "operations": [
    {
      "op_id": "op-0001",
      "action": "move",
      "source": "/mnt/nvme0/ai/models/sd15/v1-5-pruned-emaonly.safetensors",
      "destination": "/mnt/usbc1/models/checkpoints/v1-5-pruned-emaonly.safetensors",
      "file_id": "f-0001",
      "size_bytes": 2132803584,
      "size_human": "1.99 GB",
      "selected": true,
      "status": "pending",
      "symlinks_to_update": []
    }
  ],
  "space_analysis": {
    "target_drive_free_before": 3800000000000,
    "total_move_size": 2132803584,
    "target_drive_free_after": 3797867196416,
    "source_drives_freed": {
      "/mnt/nvme0": 2132803584
    },
    "fits": true
  },
  "dry_run_log": null
}
```

---

## Appendix B — Glossary

| Term | Definition |
|---|---|
| **Mountpoint** | A directory where a filesystem (drive partition) is attached. e.g., `/mnt/nvme0` |
| **Real path** | The canonical, symlink-resolved, absolute path to a file |
| **Symlink tree** | The directory structure as applications see it, including symbolic links |
| **SafeTensors** | A safe, fast file format for storing tensors, used by Hugging Face |
| **LoRA** | Low-Rank Adaptation — small model files that modify a base model's behavior |
| **Embedding** | Textual inversion embedding — a small trained vector that teaches a model a new concept |
| **BLAKE3** | A modern cryptographic hash function, faster than SHA-256 with equivalent security |
| **Dry run** | Simulate execution without modifying any files |
| **Category** | Classification of a model file (checkpoint, lora, embedding, vae, controlnet, upscaler, other) |
| **Operation** | A single planned action (move, copy, skip, delete_duplicate) in a sort plan |

---

> **Document version**: 1.0  
> **Next step**: Approve this plan, then begin implementation starting with Phase 1 (Discovery Engine).
