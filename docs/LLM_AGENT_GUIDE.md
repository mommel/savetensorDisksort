# SaveTensor DiskSort — LLM Agent Guide

## Context for AI & LLM Coding Assistants

This document outlines key invariants, core constraints, and potential pitfalls for future LLM agents modifying or extending **SaveTensor DiskSort**.

---

## 1. Non-Negotiable Invariants

1. **Zero Data Loss Rule**:
   - Never replace `copy_with_progress` + `verify_copy` + `delete_source_file` with a direct `std::fs::rename` across distinct filesystems.
   - Source files must **never** be deleted unless `verify_copy` returns `Ok(hash)`.
   - If an error or hash mismatch occurs, retain both the source and log an error.

2. **Strict Physical vs. Logical Separation**:
   - `walker.rs` strictly traverses physical directories with `follow_links(false)`. Never enable link following in physical discovery; otherwise symlinks will duplicate file sizes and distort total disk accounting.
   - `symlink_mapper.rs` handles logical symlink graphs separately and detects cycles with inode / visited canonical path sets.

3. **Base-1024 Accounting Consistency**:
   - All human-readable sizes (GB, MB, TB) use base-1024 calculations (`1024 * 1024 * 1024` for 1 GB) as specified in the plan. Always use `crate::utils::format_bytes`.

4. **Path Normalization**:
   - JSON serialization and comparisons rely on normalized paths using `/` separators without Windows UNC prefixes (`\\?\`). Use `crate::utils::normalize_path` and `crate::utils::canonicalize_lossy`.

---

## 2. Key Files Map

| File | Purpose | Critical Details |
|---|---|---|
| `src/discovery/walker.rs` | Parallel drive scan | Rayon parallelism across mountpoints; ignores symlinks |
| `src/discovery/symlink_mapper.rs` | App symlink resolution | Cycle detection; cross-references `symlinked_from` |
| `src/accounting/size_tree.rs` | FolderTree aggregation | Bottom-up size summing at each directory depth |
| `src/planner/sort_plan.rs` | Plan generator | Capacity check (+5% safety margin), layout templates |
| `src/executor/copy.rs` | Buffered stream copy | 8MB buffer, writes to `.disksort_tmp`, calls `sync_all` |
| `src/executor/verify.rs` | Checksum verification | Compares BLAKE3 hashes of source and destination |
| `src/executor/cleanup.rs` | Original cleanup & symlinks | Deletes source only post-verify, updates symlinks |
| `src/persistence/log.rs` | Append-only journal | Writes each atomic phase for crash recovery |
| `src/tui/app.rs` | Terminal UI | 5-tab Ratatui state machine and worker threads |

---

## 3. Common Pitfalls & Gotchas

- **Disk Space Margin**: Always preserve the 5% safety margin in `SpaceAnalysis`. Do not attempt to fill a drive to 100%.
- **Large File Hashing**: Always prefer `memmap2` for files > 1 MB with fallback to buffered streaming for safety on volatile mounts.
- **Sysinfo Disks**: When calling `sysinfo::Disks::refresh(&mut self, remove_not_listed_disks: bool)`, pass `false` or appropriate flag matching `sysinfo 0.33+`.
- **Windows Symlinks**: In tests on Windows without Developer Mode, `create_symlink` may return an OS privilege error. Tests should check `if res.is_ok()` when testing symlink behavior.
