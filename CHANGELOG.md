# Changelog

All notable changes to the funny_set_exploration project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2025-12-07

### Added

- **Restart capability**: Resume processing from a specific input size and batch
  - New `--restart <SIZE> <BATCH>` CLI argument to resume interrupted processing
  - Counts existing output files to preserve accurate totals across restarts
  - Only counts files created from input batches before the restart point
- **Modular file naming system**: Enhanced filename format for better tracking
  - Format: `nsl_{source_size:02}_batch_{source_batch:05}_to_{target_size:02}_batch_{target_batch:05}.rkyv`
  - Example: `nsl_05_batch_00001_to_06_batch_00012.rkyv` (from size 5 batch 1, creates size 6 batch 12)
  - **5-digit batch numbers** for scalability (supports up to 99,999 batches)
  - **Continuous output batch numbering**: Output batch counter never resets across source files
  - When restarting, automatically continues from the highest existing batch number
  - Each input batch creates independent set of output batches
  - Unified filename helpers as single source of truth for consistency
- **Per-file statistics**: Detailed logging of input/output counts per batch file

### Fixed

- **Batch processing bug**: Fixed issue where only batch 000 was processed for each size
  - Changed `refill_current_from_file()` to search directory for matching pattern instead of constructing filename
  - Uses wildcard pattern `*_to_{size}_batch_{batch}.rkyv` to find input files
- **Restart baseline counting**: Correctly parses source batch numbers from filenames
  - Counts outputs created from source batches < restart batch (allows reprocessing)
- **Naming collision prevention**: Output batches numbered continuously to avoid confusion
  - Old: `nsl_05_batch_000_to_06_batch_005` and `nsl_05_batch_001_to_06_batch_005` would collide
  - New: `nsl_05_batch_00000_to_06_batch_00005` followed by `nsl_05_batch_00001_to_06_batch_00017`

### Changed

- Enhanced output formatting with thousand separators (16-character padding)
- Improved debug logging for file operations and batch processing
- Updated CLI to support restart mode alongside size range mode

## [0.3.2] - 2025-12-07

### Changed

- **Simplified to single hybrid implementation**: Removed v0.2.2 (full heap) and v0.3.0 (full stack) implementations
  - Only v0.3.1 hybrid approach remains as the production implementation
  - Removed `-v/--version` CLI flag - no longer needed with single implementation
  - Simplified codebase by removing `list_of_nlists.rs` (v0.2.2) and `list_of_nsl.rs` (v0.3.0)
- **Module refactoring for clarity**:
  - Renamed `NList` → `NoSetListSerialized` to clarify its role as a serialization format
  - Renamed `nlist.rs` → `no_set_list_serialized.rs`
  - Updated conversion methods: `from_nlist()`/`to_nlist()` → `from_serialized()`/`to_serialized()`
  - Renamed `ListOfNSLHybrid` → `ListOfNlist`
  - Renamed `list_of_nsl_hybrid.rs` → `list_of_nlists.rs`
- **File naming simplified**: `nlist_v31_*` → `nlist_*` (removed version prefix)
- **Updated CLI interface**:

  ```bash
  # Before (v0.3.1):
  funny.exe -v 31 --size 5-7 -o T:\data
  
  # After (v0.3.2):
  funny.exe --size 5-7 -o T:\data
  ```

### Architecture

**Current Implementation (Hybrid Only):**

- **NoSetList** (stack-based, fixed arrays): Fast computation with zero heap allocations
- **NoSetListSerialized** (heap-based, Vec): Compact I/O format (~2GB per 20M batch)
- **Conversion**: Explicit `to_serialized()`/`from_serialized()` between formats
- **Performance**: 4-5× faster computation than v0.2.2, same compact file size

### Removed

- v0.2.2 heap-based implementation (was: slower but working)
- v0.3.0 stack-only implementation (was: fast but 15GB files)
- Multi-version CLI dispatch code
- Legacy `build_higher_nlists()` method (unused since v0.3.1)

## [0.3.1] - 2025-12-07

### Added

- **Hybrid implementation combining best of v0.2.2 and v0.3.0**:
  - Uses `NoSetList` (stack arrays) for 4-5× faster computation
  - Converts to `NList` (heap Vec) for compact 2GB files
  - Best of both worlds: fast + compact
- **Detailed timing breakdown**:
  - `computation_time`: Core algorithm execution (stack operations)
  - `file_io_time`: File read/write operations
  - `conversion_time`: NoSetList ↔ NList transformations
- **File format**: `.rkyv` files with version prefix `nlist_v31_*`

### Performance

**v0.3.1 vs v0.2.2 (size 6):**

- Total time: 308s vs 398s (~23% faster)
- Computation: 6.2% vs 53-57% (10× less time in algorithm)
- File I/O: 54% vs 32-40% (larger share due to faster compute)
- Conversion: 12.1% (new overhead, acceptable)
- File size: ~2GB (same as v0.2.2)

### Technical Details

- Hybrid conversion adds ~12% overhead but worth it for compact files
- Memory-mapped I/O still used for zero-copy deserialization
- rkyv size_32 encoding maintains 2GB file sizes
- Compatible file format with v0.2.2 (both use NList for I/O)

## [0.3.0] - 2025-12-06

### Added

- **Full stack-optimized implementation**: Zero heap allocation during computation
  - New `NoSetList` struct with fixed-size arrays (`[usize; 18]`, `[usize; 78]`)
  - Implements `Copy` trait for efficient stack operations
  - New `build_higher_nsl()` method using in-place stack manipulations
  - New file format: `.nsl` files (stack-serialized format)
- **Separate module**: `list_of_nsl.rs` for stack-only processing
- **Version dispatch**: CLI `-v 3` flag to select stack implementation

### Performance

**v0.3.0 vs v0.2.2 (size 5):**

- Total time: 63s vs 380-426s (6-7× faster)
- Computation: 2.3% vs 56% (25× faster algorithm)
- File I/O: 89.8% vs 32-36% (became the bottleneck)

**Trade-offs:**

- ✅ 4-8× faster computation (zero malloc overhead)
- ✅ Better cache locality with stack data
- ❌ 7-8× larger files (~15GB vs ~2GB per batch)
- ❌ High I/O overhead negates computation gains

### Technical Details

- Fixed-size arrays serialize full 768 bytes per entry
- rkyv with arrays: no length compression like Vec
- Bottleneck shifted from computation to I/O
- Led to development of hybrid v0.3.1 approach

## [0.2.2] - 2025-12-06

### Added

- **CLI support with optional arguments**: Added command-line interface using `clap`
  - `--size` / `-s` option to build specific sizes (4-12)
  - `--output-path` / `-o` option to specify custom output directory
  - Help documentation accessible via `--help`
- Default behavior preserved when no arguments provided
- `CLI_USAGE.md` - Comprehensive CLI usage guide with examples

### Changed

- `main.rs` now supports dual modes: default behavior (no args) or CLI mode (with args)
- Added `clap = { version = "4.5", features = ["derive"] }` dependency

### Fixed

- Toolchain configuration: Fixed GNU toolchain compatibility
  - Removed conflicting LLVM MinGW package
  - Installed WinLibs MinGW-w64 with proper GCC libraries
  - Resolved linker errors (`libgcc`, `libgcc_eh`, `dlltool.exe`)

### Notes

- Executable is self-contained and can be copied/run from any location
- Added `*.exe` to `.gitignore`

## [0.2.1] - 2025-12-06

### Added

- **Zero-copy serialization with rkyv**: Migrated from bincode to rkyv for dramatically improved performance
  - Memory-mapped file support using `memmap2` for zero-copy deserialization
  - 10-100x faster file read operations
  - ~50% reduction in peak memory usage (4-5GB vs previous 13.5GB)
  - Validation with `check_archived_root` for safe archived data access
- New file format: `.rkyv` files (replacing `.bin` files)
- Backward compatibility: Automatic fallback to bincode for existing `.bin` files
- Comprehensive documentation:
  - `RKYV_IMPLEMENTATION.md` - Technical implementation details and code examples
  - `RKYV_MIGRATION.md` - Migration guide and testing procedures
  - `RKYV_COMPLETE.md` - Implementation summary and status

### Changed

- File extension changed from `.bin` to `.rkyv` for new files
- `save_to_file()` now uses rkyv serialization (legacy `save_to_file_bincode()` kept for compatibility)
- `read_from_file()` now uses memory-mapped rkyv deserialization with automatic bincode fallback
- `NList` structure enhanced with rkyv derives (`Archive`, `Serialize`, `Deserialize`)
- Updated `src/main.rs` documentation to reflect new memory characteristics

### Technical Details

- Added `rkyv = { version = "0.7", features = ["validation", "size_32"] }` dependency
- Added `memmap2 = "0.9"` for memory-mapped file I/O
- Kept `serde` and `bincode` dependencies for backward compatibility
- Files are validated before access using `check_archived_root()`
- Unsafe blocks properly contained with validation checks

### Performance Improvements

- Read speed: 10-50x faster (0.1-0.5 sec vs 3-5 sec for 4GB files)
- Peak memory: 63% reduction (4-5GB vs 13.5GB)
- File size: ~5-10% larger than bincode (acceptable trade-off for performance gains)

## [0.2.0] - 2025-12-06

### Added

- **Configurable output directory**: Added support for custom base paths for file I/O operations
  - New `base_path` field in `ListOfNlist` struct to store directory path
  - New `with_path()` constructor to create `ListOfNlist` with custom output directory
  - Support for Windows paths (local drives, mapped network drives, UNC paths)
  - Support for Linux/macOS paths (absolute paths, NAS mount points, relative paths)
  - Cross-platform path handling using `std::path::Path`
- Documentation for path configuration (`PATH_CONFIGURATION.md`)
- Example file demonstrating path usage patterns (`examples/path_examples.rs`)

### Changed

- Modified `filename()` function to accept `base_path` parameter and construct full paths
- Updated all file operations (`save_to_file`, `read_from_file`, `refill_current_from_file`) to use base path
- `ListOfNlist::new()` now defaults to current directory (".")
- Improved in-code documentation with examples for Windows and Linux path syntax

### Technical Details

- Files are no longer saved only in the project root directory
- Users can now specify output locations like NAS drives (e.g., `T:\data\funny_set_exploration` on Windows)
- The `base_path` field is marked with `#[serde(skip)]` to avoid serialization

## [0.1.0] - 2025-12-04 to 2025-12-05

### Added

- Initial working implementation of the n-list exploration algorithm
- Core modules:
  - `set.rs`: Set validation logic and card operations
  - `nlist.rs`: N-list structure and operations
  - `list_of_nlists.rs`: Batch processing and file I/O
  - `utils.rs`: Debug and utility functions
- Batch file processing with configurable limits (`MAX_NLISTS_PER_FILE`)
- Binary serialization using `bincode` for efficient storage
- Progress tracking and formatted output with `separator` crate
- File batching system to manage large datasets (20 million n-lists per file, ~4GB each)

### Implementation Milestones

- 2025-12-05: Code linting and structure improvements
- 2025-12-05: Added count tracking for newly created n-lists
- 2025-12-04: Fixed bug where final batch of n-lists wasn't being saved
- 2025-12-04: Implemented batching system for file I/O operations
- 2025-12-04: Added serialization support for n-list persistence
- 2025-12-04: Expanded algorithm to generate no-set-4 lists
- 2025-12-04: Implemented n+1 list construction from n-lists
- 2025-12-04: Created foundational data structures and functions

### Algorithm Features

- Seed list generation: Creates all valid 3-card combinations with no sets
- Incremental expansion: Builds n+1-lists from n-lists iteratively
- Optimization: Only explores cards with values greater than current maximum
- Forbidden card tracking: Efficiently filters out cards that would form sets
- Memory management: Batch processing to handle datasets that exceed RAM capacity

## [0.0.1] - 2025-11-30 to 2025-12-01

### Added

- Initial repository setup
- Project structure and build configuration
- Comprehensive README documenting:
  - Algorithm principles and optimization strategies
  - Implementation approach (seed lists, incremental growth)
  - Future optimization ideas (symmetry reduction)
- Basic exploration strategy outline

### Project Goals

- Find all combinations of 12, 15, and 18 cards with no valid sets
- Exhaustive search with optimized algorithms
- Handle massive combination spaces efficiently

## Version History Summary

- **v0.2.2** (2025-12-06): CLI support with optional arguments
- **v0.2.1** (2025-12-06): Zero-copy serialization with rkyv
- **v0.2.0** (2025-12-06): Configurable output directories for file storage
- **v0.1.0** (2025-12-04/05): Core implementation with batch processing
- **v0.0.1** (2025-11-30): Initial project setup and documentation

---

## Future Considerations

### Planned Enhancements

- **Serialization Migration**: Consider replacing `bincode` with `rkyv` for:
  - Zero-copy deserialization (10-100x faster reads)
  - Reduced memory usage during file loading
  - Memory-mapped file support for huge datasets
  - Expected benefits: Lower peak RAM usage (~4-5GB vs current ~13.5GB)

### Performance Optimizations

- Symmetry reduction using card rotation properties
- Parallel processing for independent n-list expansions
- GPU acceleration for set validation operations

### Features Under Consideration

- Progress persistence (checkpoint/resume capability)
- Multi-threaded file I/O
- Compressed storage formats
- Analysis tools for generated n-lists
- Visualization of results

---

## Notes

- Each batch file is approximately 4GB in size
- Typical RAM usage peaks at ~13.5GB when batch is being saved
- After saving, RAM usage drops to ~5GB
- Default batch size: 20 million n-lists per file
