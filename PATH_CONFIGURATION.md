# Path Configuration Guide

## Overview

The `funny_set_exploration` program now supports configurable output directories for saving and reading n-list batch files. This allows you to store the large data files on external drives, NAS storage, or any custom location.

## How to Configure the Output Path

In `src/main.rs`, you can choose where files are saved by using one of two methods:

### Method 1: Default (Current Directory)

```rust
let mut no_set_lists: ListOfNlist = ListOfNlist::new();
```

Files will be saved in the current directory where you run the program.

### Method 2: Custom Path

```rust
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path("your/path/here");
```

Specify any directory path where you want the files saved.

## Valid Path Examples

### Windows

```rust
// Absolute path with drive letter
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path(r"C:\data\funny_set");

// Network/NAS drive mapped to a letter (e.g., T:)
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path(r"T:\data\funny_set_exploration");

// UNC network path
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path(r"\\server\share\funny_set");

// Relative path
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path(r"output\data");
```

**Note:** The `r"..."` prefix creates a "raw string" in Rust, which treats backslashes `\` as literal characters (not escape sequences). This is recommended for Windows paths.

### Linux / macOS

```rust
// Absolute path
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path("/home/user/data/funny_set");

// NAS mounted directory
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path("/mnt/nas/data/funny_set_exploration");

// Network File System (NFS)
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path("/nfs/storage/funny_set");

// Relative path
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path("output/data");
```

### Cross-Platform Relative Paths

```rust
// Subdirectory in current folder (works on all platforms)
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path("output");

// Nested subdirectory
let mut no_set_lists: ListOfNlist = ListOfNlist::with_path("data/nlists");
```

## Important Notes

1. **Directory Must Exist**: Make sure the directory exists before running the program. The program does not automatically create directories.

2. **Permissions**: Ensure you have write permissions for the target directory.

3. **Disk Space**: Each batch file can be ~4GB. Make sure your target drive has sufficient space.

4. **Performance**:
   - Local SSD: Best performance
   - Local HDD: Good performance
   - Network/NAS: May be slower, depends on network speed

## Example Usage Scenario

If you have a NAS drive mapped to `T:\data\funny_set_exploration` on Windows:

1. Ensure the directory exists:

   ```powershell
   New-Item -ItemType Directory -Path "T:\data\funny_set_exploration" -Force
   ```

2. In `src/main.rs`, uncomment and modify:

   ```rust
   let mut no_set_lists: ListOfNlist = ListOfNlist::with_path(r"T:\data\funny_set_exploration");
   ```

3. Run your program normally:

   ```bash
   cargo run --release
   ```

4. Files will be saved as:
   - `T:\data\funny_set_exploration\nlist_03_batch_000.bin`
   - `T:\data\funny_set_exploration\nlist_04_batch_000.bin`
   - etc.

## File Naming Convention

Files are automatically named using this pattern:

```
nlist_{size:02}_batch_{number:03}.bin
```

Examples:

- `nlist_03_batch_000.bin` - First batch of 3-card lists
- `nlist_04_batch_000.bin` - First batch of 4-card lists
- `nlist_06_batch_042.bin` - 43rd batch of 6-card lists

## Troubleshooting

**Error: No such file or directory**

- The directory doesn't exist. Create it first.

**Error: Permission denied**

- You don't have write permissions. Check folder permissions.

**Files not appearing**

- Verify the path is correct
- Check that the program completed successfully
- Ensure no typos in the path string

**Network drive issues**

- Ensure the drive is mounted/mapped correctly
- Check network connectivity
- Try a local path first to verify the program works
