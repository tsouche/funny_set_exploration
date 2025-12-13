# Changelog

All notable changes to the funny_set_exploration project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.6] - 2025-12-13

### Added

- **Input-intermediary file generation**: Automatic tracking of output files created from each input batch
  - New file format: `no_set_list_input_intermediate_count_{size:02}_{input_batch:04}.txt`
  - Generated automatically during `--size`, `--restart`, and `--unitary` modes
  - Contains one line per output file created from the input batch
  - Format: `   ... {count:>8} lists in {output_filename}`
  - Enables precise tracking of which output files were generated from which input batches
  - Improves restart capability and repository integrity verification

- **Enhanced count mode**: Leverages input-intermediary files for efficiency
  - First checks for existing input-intermediary files before reading .rkyv files
  - Creates missing input-intermediary files on-demand (idempotent operation)
  - Groups files by source batch for organized processing
  - Shows progress for both valid (skipped) and created files
  - Significantly faster when input-intermediary files exist (no need to read large .rkyv files)

### Changed

- **Atomic file writes**: Input-intermediary files now written atomically
  - Lines buffered in memory during batch processing
  - File written in single operation only after all output files are successfully created
  - If process interrupted mid-batch, no incomplete file is created
  - Ensures file existence indicates complete batch processing
  - Applies to both generation modes and count mode

- **Improved output formatting**: Cleaner progress messages
  - Removed redundant batch headers during processing
  - Single "saving input intermediary file" message per batch
  - Blank lines strategically placed for readability
  - Count mode shows which batches are skipped vs. created
  - Consistent formatting between generation and count modes

### Technical Details

**Input-Intermediary File System:**
- **Purpose**: Track which output batches were created from each input batch
- **Location**: Stored in output directory alongside output files
- **Naming**: `no_set_list_input_intermediate_count_{source_size:02}_{source_batch:04}.txt`
- **Content**: One line per output file, sorted by output batch number
- **Validation**: Count mode verifies file completeness (line count matches output file count)
- **Recovery**: Missing or incomplete files automatically recreated by count mode

**Implementation:**
- Added `input_intermediary_buffer` field to `ListOfNSL` struct
- New methods: `buffer_input_intermediary_line()`, `write_input_intermediary_file()`
- Modified `save_new_to_file()` to buffer output file information
- Modified `process_batch_loop()` to write buffered lines atomically
- Count mode groups files by source batch for efficient processing

## [0.4.5] - 2025-12-13

### Changed

- **Extended batch numbering capacity**: Updated intermediary count files to support 4-digit batch numbers
  - Changed format from `no_set_list_intermediate_count_{size:02}_{batch:03}.txt` to `{batch:04}.txt`
  - Required for sizes 10+ which generate >1200 batches (11544 files ÷ 10 = 1155 batches)
  - Batch display messages also updated from `{:03}` to `{:04}` format
  - Impact: Count mode now properly handles large file collections without numbering overflow

## [0.4.4] - 2025-12-13

### Fixed

- **Size mode visibility**: Added consistent logging for file loading progress
  - Size mode now displays "loading batch X" and "loaded Y lists" messages
  - Matches behavior of restart and unitary modes for better user feedback
  - Previously used debug_print (hidden), now uses test_print (visible)

- **Unicode encoding issues**: Replaced Unicode symbols with ASCII-safe alternatives
  - Changed ✓ to `[OK]` and ✗ to `[!!]` in check mode output
  - Fixes display corruption in Windows PowerShell (was showing "Γ£ô")
  - All status indicators now display correctly across all terminal types

- **Path resolution bug**: Fixed --size mode not respecting -i/--input-path
  - Size mode was reading from output directory instead of input directory
  - Now correctly uses input_path for reading and output_path for writing
  - Previously only --restart and --unitary modes handled dual paths correctly

### Changed

- **Code refactoring (Phases 1-3)**: Major reduction in code duplication
  - **Phase 1**: Extracted common helper functions (validate_size, resolve_paths, handle_force_recount)
  - **Phase 2**: Created unified ProcessingConfig structure and ProcessingMode enum
  - **Phase 3**: Restructured main() to use centralized execute_mode() dispatcher
  - Reduced main.rs from 637 lines to 579 lines (9% reduction)
  - All modes now use consistent path resolution and validation logic
  
- **list_of_nsl.rs refactoring**: Eliminated duplication in processing methods
  - Added helper methods: init_processing_state(), init_output_batch(), print_timing_report(), process_batch_loop()
  - Refactored process_all_files_of_current_size_n() from 67 to 28 lines (58% reduction)
  - Refactored process_from_batch() from 73 to 25 lines (66% reduction)
  - Refactored process_single_batch() from 69 to 32 lines (54% reduction)
  - All three processing modes now share common initialization and reporting logic
  - Consistent test_print() usage across all modes for file loading messages

### Improved

- **Code maintainability**: Single source of truth for common operations
  - Path resolution logic unified across all 7 modes
  - Size validation centralized with mode-specific ranges
  - Timing reports use identical formatting across all modes
  - Easier to add new modes or modify existing behavior

## [0.4.3] - 2025-12-11

### Added

- **Check mode**: New `--check <SIZE>` command to verify repository integrity
  - Scans directory for missing batches (checks continuous numbering)
  - Validates consolidated count file against actual files
  - Validates intermediary count files against actual files
  - Three-tier verification: batch sequence → consolidated → intermediary files
  - Example: `--check 8 -o .\07_to_08\`
  - Provides clear [OK]/[!!] indicators for each validation step
  - Helps identify missing or corrupted files before processing

### Changed

- **Refactored intermediary file format**: Human-readable one-line-per-file format
  - Old format: `source_batch target_batch count filename` (whitespace-separated)
  - New format: `"   ... count lists in filename"` (matches test_print output)
  - Easier to read and verify manually
  - Consistent with user-facing progress messages
  - Example: `"   ... 10,000,003 lists in nsl_08_batch_00002_to_09_batch_00010.rkyv"`
  - Both writing (process_count_batch) and reading (consolidate_count_files) updated

### Fixed

- **Intermediary filename bug**: Removed accidental prefix in count mode
  - Issue: Intermediary filenames had `"   ... "` prepended to path
  - Result: Invalid paths like `"   ... .\07_to_08\/no_set_list_intermediate_count_08_0000.txt"`
  - Solution: Removed formatting prefix from filename construction
  - Impact: Count mode now properly creates and checks intermediary files

## [0.4.2] - 2025-12-11

### Changed

- **Optimized restart mode batch numbering**: Implemented Method 3 (filename-only scanning)
  - Removed slow file deserialization methods (Method 1: count file + Method 2: deserialize all files)
  - New approach: Scans output directory filenames to extract highest batch number
  - Performance: Milliseconds instead of minutes/hours for large file collections
  - Net code reduction: -166 lines (removed old methods) +62 lines (new method) = -104 lines
  - Function: `get_next_output_batch_from_files()` - O(n) where n = number of files, not file size
- **Enhanced dual path support**: Improved separation of input/output directories
  - Restart mode now properly uses `-i` for input files and `-o` for output files
  - Input files remain in source directory (read-only)
  - Output files written to separate target directory
  - Enables safer processing with separate source/target locations
- **Improved count mode robustness**: Batched processing with idempotency
  - Processes files in batches of 10 (COUNT_BATCH_SIZE)
  - Creates intermediary tracking files: `no_set_list_intermediate_count_{size:02}_{batch:04}.txt`
  - Timestamp-based idempotency: Skips batches if intermediary file is newer than source files
  - Handles thousands of files efficiently without performance degradation
  - Intermediary files kept for debugging and restart capability

### Fixed

- **Restart mode file generation**: Fixed critical bug where restart mode wasn't creating output files
  - Issue: Processing completed but no output files were generated
  - Root cause: Extensive debug output during processing loop severely impacted performance
  - Solution: Controlled debug output with `debug_print_off()` during tight processing loops
  - Result: Restart mode now successfully generates output files with clean user-facing messages

## [0.4.1] - 2025-12-10

### Changed

- **Renamed `--unit` to `--unitary`**: More descriptive mode name
  - Updated all documentation to reflect new terminology
  - Emphasized that unitary mode is the ONLY canonical way to overwrite/fix defective files
  - Updated CLI arguments, function names, and mode descriptions
- **Improved count file format**: Better column organization
  - Swapped column order: `cumulative_nb_lists` now comes before `nb_lists_in_file`
  - Renamed columns for clarity:
    - `lists_in_file` → `nb_lists_in_file`
    - `cumulative_total` → `cumulative_nb_lists`
  - New format: `source_batch target_batch | cumulative_nb_lists | nb_lists_in_file | filename`
- **Renamed `--audit` to `--count`**: More intuitive terminology throughout
  - Renamed `audit_size_files()` → `count_size_files()`
  - Updated all documentation and help text
- **Renamed `--replay` to `--unit` then to `--unitary`**: Progressive refinement of terminology
  - Clarified the single-batch processing concept
  - Better distinguishes from restart mode (continues onwards) vs unitary (single batch only)

## [0.4.0] - 2025-12-07

### Added

- **Unitary mode**: New `--unitary <SIZE> <BATCH>` command to reprocess a single input batch
  - SIZE refers to INPUT size (same as restart semantics)
  - Reprocesses only the specified batch, regenerating its output files
  - This is the ONLY canonical way to overwrite/fix defective output files
  - Example: `--unitary 5 2` reprocesses input size 5 batch 2 (creates size 6 outputs)
  - Uses count file for baseline (like restart mode)
  - Supports `--force` flag to regenerate count file first
  - Useful for fixing corrupted output files or after algorithm changes
- **Count mode**: New `--count <SIZE>` command to count existing files without processing
  - Scans all output files for a given target size
  - Counts lists in each file
  - Creates summary report: `no_set_list_count_XX.txt`
  - Report format: source_batch, target_batch, cumulative_nb_lists, nb_lists_in_file, filename
  - Files listed in descending batch order (highest first)
  - Useful for verifying counts and tracking progress
- **Restart capability**: Resume processing from a specific input size and batch
  - New `--restart <SIZE> <BATCH>` CLI argument to resume interrupted processing
  - Reads baseline counts from count file (fast, no file scanning)
  - New `--force` flag to regenerate count file before restart (scans all files)
  - Example: `--restart 5 1` uses existing count file
  - Example: `--restart 5 1 --force` regenerates count file first
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
