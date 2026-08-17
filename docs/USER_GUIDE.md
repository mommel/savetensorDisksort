# SaveTensor DiskSort — User Guide

## Introduction

**SaveTensor DiskSort** brings order to scattered AI model collections across multiple internal and external (USB-C) drives. It discovers real files, maps existing symlinks, calculates precise disk usage in GB, generates a clean sort plan, and safely relocates models with automatic verification and symlink redirection.

---

## 1. Quick Start

### Installation
Build the binary locally using Cargo:
```bash
cargo build --release
# The single binary is produced at target/release/disksort (or disksort.exe on Windows)
```

---

## 2. Configuration (`disksort.json`)

Create a `disksort.json` file in your working directory (or specify `--config <path>`):

```json
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
  "output_dir": "./disksort_data",
  "verify_after_copy": true,
  "update_symlinks_after_move": true
}
```

---

## 3. CLI Workflow

### Step 1: Scan Drives
Scan physical drives and map application symlinks:
```bash
disksort scan --mountpoints /mnt/nvme0,/mnt/usbc1 --app-roots /home/user/ComfyUI
```
Output is saved to `disksort_data/inventory.json`.

### Step 2: Inspect Inventory
View summaries, duplicate files, and per-drive disk usage:
```bash
# High-level summary
disksort inventory

# Duplicate candidates across drives
disksort inventory --duplicates

# Breakdown by storage drive
disksort inventory --by-drive
```

### Step 3: Generate a Sort Plan
Choose a target destination drive and folder template:
```bash
# Generate plan using the 'by-type' template
disksort plan --target /mnt/usbc1 --template by-type

# Generate plan sub-categorized by base architecture (SD1.5, SDXL, Flux, Pony)
disksort plan --target /mnt/usbc1 --template by-type-and-base
```
Output is written to `disksort_data/sort_plan.json`. You can review or edit this JSON file directly if you wish to adjust specific destination paths!

### Step 4: Dry-Run
Simulate the planned operations without modifying or copying any files:
```bash
disksort sort --dry-run
```

### Step 5: Execute Real Relocation
Perform safe, verified relocation:
```bash
disksort sort --execute
```
During execution, DiskSort:
1. Copies each model to a temporary file (`.disksort_tmp`).
2. Calculates and compares BLAKE3 hashes for bit-for-bit equivalence.
3. Deletes the original file **only** after verification passes.
4. Atomically redirects all symlinks in ComfyUI / WebUI to the new target.
5. Logs each step to `execution.jsonl` for full crash safety.

### Step 6: Health Check (`doctor`)
Check for broken symlinks and orphaned model references:
```bash
disksort doctor
```

---

## 4. Interactive Terminal User Interface (TUI)

Launch the interactive 5-tab TUI at any time:
```bash
disksort tui
```

### Key Bindings
| Key | Action |
|---|---|
| `Tab` / `BackTab` | Cycle between tabs (Scan, Inventory, Plan, Execute, Summary) |
| `↑` / `↓` (`k` / `j`) | Navigate tree and operation lists |
| `Enter` / `→` | Expand folder in Inventory |
| `←` | Collapse folder in Inventory |
| `Space` | Toggle checkbox `[x]` / `[ ]` for an operation in Plan view |
| `a` | Select All / Deselect All operations in Plan view |
| `d` | Start Dry-Run simulation |
| `x` | Start Real Execution (Copy -> Verify -> Delete) |
| `r` | Refresh / Rescan |
| `q` | Quit application |
