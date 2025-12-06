# CLI Usage Guide

## Overview

The program now supports both **default mode** and **CLI mode** for flexible operation.

## Running the Program

### Default Mode (No Arguments)

Runs the original behavior: creates seed lists and processes sizes 4-6.

```powershell
cargo run
```

This will:

1. Create seed lists (size 3)
2. Generate size 4 from size 3
3. Generate size 5 from size 4
4. Generate size 6 from size 5

### CLI Mode (With --size Argument)

Process a specific size only.

```powershell
# Build size 5 from existing size 4 files
cargo run -- --size 5

# Build size 4 (will create seed lists first if needed)
cargo run -- --size 4

# Build size 7 from existing size 6 files
cargo run -- --size 7
```

### Custom Output Directory

You can specify a custom output directory with `-o` or `--output-path`:

```powershell
# Windows: Use NAS drive
cargo run -- --size 5 -o "T:\data\funny_set_exploration"

# Linux: Use mounted NAS
cargo run -- --size 5 -o "/mnt/nas/data/funny_set_exploration"

# Relative path
cargo run -- --size 5 -o "./output"
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
