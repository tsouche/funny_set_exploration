# CLI Usage Guide

## Overview

Version 0.4.1 provides four operational modes: normal size processing, restart from specific batch, unitary processing (single batch - the only canonical way to fix defective files), and count mode. The program uses a hybrid stack/heap implementation for optimal performance.

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
```

### CLI Mode: Size Range (v0.4.1+)

Process multiple sizes in a single run:

```powershell
# Build sizes 5, 6, and 7 sequentially
cargo run --release -- --size 5-7

# Build sizes 8 through 10
cargo run --release -- --size 8-10

# Extended range (up to size 18)
cargo run --release -- --size 10-12
```

### Custom Output Directory

You can specify a custom output directory with `-o` or `--output-path`:

```powershell
# Windows: Use NAS drive
cargo run --release -- --size 7 -o "T:\data\funny_set_exploration"

# Linux: Use mounted NAS
cargo run --release -- --size 7 -o "/mnt/nas/data/funny_set_exploration"

# Relative path
cargo run --release -- --size 5-7 -o "./output"
```

## Command-Line Options

```text
funny_set_exploration [OPTIONS]

Options:
  -s, --size <SIZE>
          Target size to build (4-18 or range like 5-7)
          
          If not provided, runs the default behavior (creates seeds + sizes 4-18)
          - Single size: "5" builds size 5 from size 4 files
          - Range: "5-7" builds sizes 5, 6, and 7 sequentially
          - Size 4: Builds from seed lists (size 3)
          - Size 5+: Requires files from previous size

      --restart <SIZE> <BATCH>
          Restart from specific input batch, continue through size 18
          SIZE refers to INPUT size
          Reads baseline from count file (use --force to regenerate)

      --unitary <SIZE> <BATCH>
          Process only one specific input batch (unitary processing)
          This is the ONLY canonical way to overwrite/fix defective files
          SIZE refers to INPUT size
          Use --force to regenerate count file first

      --count <SIZE>
          Count existing files for target size
          Creates no_set_list_count_XX.txt summary report

      --force
          Force regeneration of count file (use with --restart or --unitary)

  -o, --output-path <OUTPUT_PATH>
          Output directory path (optional)
          
          Examples:
            Windows: T:\data\funny_set_exploration
            Linux:   /mnt/nas/data/funny_set_exploration
            Relative: ./output

  -h, --help
          Print help
```

### Restart Mode

Resume processing from a specific batch:

```powershell
# Restart from input size 5 batch 2, continue through size 18
cargo run --release -- --restart 5 2 -o "T:\data\funny_set_exploration"

# Force regenerate count file before restart
cargo run --release -- --restart 5 2 --force -o "T:\data\funny_set_exploration"
```

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
# Count all size 6 files, create no_set_list_count_06.txt
cargo run --release -- --count 6 -o "T:\data\funny_set_exploration"
```

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
