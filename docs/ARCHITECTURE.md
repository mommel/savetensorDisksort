# SaveTensor DiskSort — Architecture & Technical Design

## 1. System Overview

**SaveTensor DiskSort** is a native, zero-runtime CLI and TUI tool designed to inventory, analyze, plan, and safely relocate multi-gigabyte AI model files (`.safetensors`, `.pt`, `.pth`, `.ckpt`, `.bin`, `.gguf`, `.onnx`) across multiple physical storage drives and filesystems.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        CLI / TUI Interface Layer                        │
│             `disksort scan` | `plan` | `sort` | `inventory` | `tui`     │
├─────────────────────────────────────────────────────────────────────────┤
│                               Core Modules                              │
│                                                                         │
│  ┌─────────────────┐   ┌─────────────────┐   ┌───────────────────────┐  │
│  │    Discovery    │   │   Accounting    │   │     Sort Planner      │  │
│  │ Physical walker │   │ Hierarchical    │   │ Target templates      │  │
│  │ Symlink mapper  │   │ size tree &     │   │ Capacity check (+5%)  │  │
│  │ Extension/cat   │   │ duplicate finder│   │ Collision resolution  │  │
│  └────────┬────────┘   └────────┬────────┘   └───────────┬───────────┘  │
│           │                     │                        │              │
│           └──────────────┬──────┴────────────────────────┘              │
│                          ▼                                              │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                         Execution Engine                          │  │
│  │  1. 8MB Buffered Copy (tmp file)                                  │  │
│  │  2. BLAKE3 Verification (Byte-for-byte check)                     │  │
│  │  3. Delete Source ONLY on hash match                              │  │
│  │  4. Atomic Symlink Redirection (Link -> New Target)               │  │
│  │  5. Crash-recovery journal (`execution.jsonl`)                    │  │
│  └───────────────────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────────┤
│                           Persistence Layer                             │
│       `inventory.json`  ·  `sort_plan.json`  ·  `execution.jsonl`       │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Core Architectural Pillars

### 2.1 Copy-First Execution (Zero Data Loss Mandate)
- Files are never moved using filesystem `mv` / `rename` across drive boundaries.
- Instead, files are copied to `.disksort_tmp` temporary destination files, flushed and synced (`fsync`), then atomically renamed.
- Both source and destination files are hashed using **BLAKE3**.
- The source file is deleted **if and only if** the hashes match bit-for-bit.
- If verification fails or is interrupted, the source file remains completely untouched.

### 2.2 BLAKE3 High-Performance Cryptographic Hashing
- AI model files regularly range from 2 GB to 30+ GB.
- Standard SHA-256 takes significant time on multi-terabyte collections.
- **BLAKE3** is utilized with `memmap2` and multi-threaded SIMD acceleration, yielding near-hardware memory transfer speeds.
- Safe streaming fallback (8 MB buffer) ensures rock-solid stability even on volatile USB mounts.

### 2.3 Symlink Mapping & Atomic Redirection
- Physical scan ignores symlinks to avoid duplicate counting and circular paths.
- Application roots (ComfyUI, WebUI, Forge, Kohya) are scanned separately to resolve the application-visible logical hierarchy.
- When a model file moves from Drive A to Drive B, every symlink in `symlinked_from` is atomically updated to point to the new destination.

### 2.4 State Persistence & Crash Recovery
- **`inventory.json`**: Complete snapshot of physical files, mountpoint metadata, folder hierarchy sizes, and symlink trees.
- **`sort_plan.json`**: Deterministic execution blueprint with pre-calculated destination paths and space requirements.
- **`execution.jsonl`**: Append-only journal. If power is lost or USB is unplugged mid-sort, DiskSort inspects the journal to safely clean partial temporary files or re-verify pending operations.

---

## 3. Data Models

### 3.1 File Categories
- `Checkpoint`: Large base models / UNet checkpoints (>1 GB or in checkpoints folder).
- `Lora`: Low-Rank Adaptations (50 MB–350 MB safetensors or in loras folder).
- `Embedding`: Textual Inversion tokens (<20 MB `.pt` or `.safetensors`).
- `Vae`: Variational Autoencoders.
- `Controlnet`: ControlNet conditioning adapters.
- `Upscaler`: ESRGAN, RealESRGAN, SwinIR upscaling models.
- `Other`: Miscellaneous model weights.

### 3.2 Layout Templates
1. **`by-type`** (Default): Groups files into `models/checkpoints/`, `models/loras/`, `models/embeddings/`, `models/vae/`, etc.
2. **`by-type-and-base`**: Sub-divides checkpoints and loras by base architecture: `sd15/`, `sdxl/`, `flux/`, `sd3/`, `pony/`, `general/`.
3. **`flat`**: Moves all selected models directly into `models/`.
4. **`preserve`**: Preserves relative folder paths from the source mountpoint.
