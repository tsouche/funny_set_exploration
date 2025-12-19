# CLI Usage Guide

## Overview

Version 0.4.12 provides four operational modes: size processing (with optional restart from specific batch), unitary processing (single batch - the only canonical way to fix defective files), count mode, and compact mode for consolidating small files. The program uses a hybrid stack/heap implementation with GlobalFileState for tracking.

**New in 0.4.12**: 
- Automatic compaction workflow for sizes 13+
- Compacted file recognition (*_compacted.rkyv)
- Smart processing: only compacted files by default (use --force for all files)
- Auto-compact input before processing and output after processing (sizes 13+)

## Running the Program

### Default Mode (No Arguments)

Runs the default behavior: creates seed lists and processes sizes 4-6.

```powershell
cargo run --release
```

This will:

1. Create seed lists (size 3)
2. Generate size 4 from size 3
3. Generate size 5 from size 4
4. Generate size 6 from size 5

### CLI Mode: Single Size

Process a specific size only.

```powershell
# Build size 5 from existing size 4 files
cargo run --release -- --size 5

# Build size 4 (will create seed lists first if needed)
cargo run --release -- --size 4

# Build size 7 from existing size 6 files
cargo run --release -- --size 7

# Restart from specific input batch (e.g., size 5 from batch 2)
cargo run --release -- --size 5 2

# Build size 14 with auto-compaction (input & output)
cargo run --release -- --size 14 -i ./12_to_13c -o ./13c_to_14c

# Build size 14 with --force (process all files, not just compacted)
cargo run --release -- --size 14 -i ./12_to_13c -o ./13c_to_14c --force
```

### Custom Output Directory

You can specify a custom output directory with `-o` or `--output-path`:

```powershell
# Windows: Use NAS drive
cargo run --release -- --size 7 -o "T:\data\funny_set_exploration"

# Linux: Use mounted NAS
cargo run --release -- --size 7 -o "/mnt/nas/data/funny_set_exploration"

# Relative path
cargo run --release -- --size 7 -o "./output"
```

## Command-Line Options

```text
funny_set_exploration [OPTIONS]

Options:
  -s, --size <SIZE> [BATCH]
          Target output size to build (3-18), optional batch to restart from
          
          If not provided, runs the default behavior (creates seeds + sizes 4-18)
          - Single size: "5" builds size 5 from size 4 files
          - With batch: "5 2" restarts from input batch 2
          - Size 3: Creates seed lists
          - Size 4: Builds from seed lists (size 3)
          - Size 5+: Requires files from previous size

      --unitary <SIZE> <BATCH>
          Process only one specific input batch (unitary processing)
          This is the ONLY canonical way to overwrite/fix defective files
          SIZE refers to INPUT size
          Use --force to regenerate count file first

      --count <SIZE>
          Count existing files for target size
          Creates nsl_{size:02}_global_count.txt summary report

      --force
          Force regeneration of count file (use with --size BATCH or --unitary)
          For sizes 13+: Process all files, not just compacted ones

  -o, --output-path <OUTPUT_PATH>
          Output directory path (optional)
          
          Examples:
            Windows: T:\data\funny_set_exploration
            Linux:   /mnt/nas/data/funny_set_exploration
            Relative: ./output

  -h, --help
          Print help
```

### Restart from Specific Batch

Resume processing from a specific input batch:

```powershell
# Restart from input batch 2 (output size 5, reads from size 4 batch 2)
cargo run --release -- --size 5 2 -i "./input" -o "./output"

# Force regenerate count file before restart
cargo run --release -- --size 5 2 --force -i "./input" -o "./output"
```

**For sizes 13+:** The restart mode will automatically:
1. Compact input files before processing
2. Only process compacted input files (unless --force is used)
3. Compact output files after processing

### Unitary Mode

Process only one specific batch (canonical way to fix defective files):

```powershell
# Reprocess input size 5 batch 2 only
cargo run --release -- --unitary 5 2 -o "T:\data\funny_set_exploration"

# Force regenerate count file first
cargo run --release -- --unitary 5 2 --force -o "T:\data\funny_set_exploration"
```

### Count Mode

Count existing files without processing:

```powershell
# Count all size 6 files, create nsl_06_global_count.txt
cargo run --release -- --count 6 -o "T:\data\funny_set_exploration"
```

### Compact Mode

Consolidate small files into larger batches:

```powershell
# Compact all size 15 files into 10M-entry batches
cargo run --release -- --compact 15 -i "X:\funny\14_to_15"

# Compact size 15 files up to batch 5000 (controlled compaction)
cargo run --release -- --compact 15 5000 -i "X:\funny\14_to_15"

# Compact size 12 files
cargo run --release -- --compact 12 -i "T:\data\funny_set_exploration"
```

This mode:

- Reads non-compacted files for the specified output size
- Consolidates them into 10M-entry batches (or smaller for final file)
- Optional max_batch parameter stops compaction after processing files up to that batch number
- Creates new files with format: `nsl_{src_size}_batch_{src_batch}_to_{tgt_size}_batch_{tgt_batch}_compacted.rkyv`
- Deletes or shrinks original files immediately after each compacted file creation
- Uses GlobalFileState for crash-safe, idempotent operation
- Runs until no more eligible files remain
- Automatically invoked by --size mode for sizes 13+

### Size 13+ Auto-Compaction Workflow

For sizes 13 and above, the `--size` mode automatically manages compaction:

```powershell
# Build size 14 - automatically compacts input and output
cargo run --release -- --size 14 -i "./12_to_13c" -o "./13c_to_14c"
```

**Workflow:**
1. **Pre-processing**: Compacts all size 13 input files in the input directory
2. **Processing**: Only processes compacted input files (batches 0 to last_compacted_batch)
3. **Post-processing**: Compacts all size 14 output files in the output directory

**To process all files (including non-compacted):**
```powershell
cargo run --release -- --size 14 -i "./12_to_13c" -o "./13c_to_14c" --force
```

The `--force` flag bypasses the compacted-only restriction and processes all available input files.

## Prerequisites

### For Size 4

- No prerequisites (will create seed lists automatically)

### For Size 5 and Above

- Requires `nlist_(size-1)_batch_*.rkyv` files to exist
- Example: To build size 7, you need `nlist_06_batch_*.rkyv` files

## Examples

### Sequential Building

Build sizes incrementally:

```powershell
# Build size 4
cargo run -- --size 4

# Build size 5 (requires size 4 files)
cargo run -- --size 5

# Build size 6 (requires size 5 files)
cargo run -- --size 6
```

### Building with Custom Path

```powershell
# All files go to T:\data\funny_set_exploration
cargo run -- --size 5 -o "T:\data\funny_set_exploration"
```

### Default Mode with Custom Path

Note: In default mode, you need to modify the code directly (uncomment the appropriate line in `main.rs`):

```rust
// In main.rs, uncomment one of these:
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path(
    r"T:\data\funny_set_exploration");
```

## Help

View all available options:

```powershell
cargo run -- --help
```

## Troubleshooting

### "Files not found" Error

- Make sure the previous size files exist
- Example: For `--size 6`, you need `nlist_05_batch_*.rkyv` files

### Directory Errors

- Ensure the output directory exists before running
- Create it manually: `mkdir "T:\data\funny_set_exploration"`

### Build Errors (dlltool.exe not found)

This is a linker issue. Solutions:

1. Use GNU toolchain: `rustup default stable-gnu`
2. Or install Visual Studio Build Tools with C++ support
