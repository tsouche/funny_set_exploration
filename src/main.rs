/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets
///
/// Version 0.4.6 - Input-intermediary file generation and atomic writes
/// Added: Automatic tracking of output files per input batch with atomic writes
/// 
/// CLI Usage:
///   funny.exe --size 5 -o T:\data\funny_set_exploration              # Build size 5 from size 4
///   funny.exe --size 5-7 -o T:\data\funny_set_exploration            # Build sizes 5, 6, and 7
///   funny.exe --restart 5 2 -i .\input -o .\output                   # Restart with separate paths
///   funny.exe --unitary 5 2 -o T:\data\funny_set_exploration         # Process only size 5 batch 2
///   funny.exe --count 6 -o T:\data\funny_set_exploration             # Count size 6 files
///   funny.exe --check 6 -o T:\data\funny_set_exploration             # Check size 6 integrity
///   funny.exe                                                        # Default mode (sizes 4-18)
///
/// Arguments:
///   --size, -s <SIZE>        Target size to build (4-18, or range like 5-7)
///                            If omitted, runs default behavior (creates seeds + sizes 4-18)
///   --restart <SIZE> <BATCH>   Restart from specific input batch through size 18
///   --unitary <SIZE> <BATCH>   Process only one specific input batch (unitary processing)
///   --count <SIZE>             Count existing files and create summary report
///   --check <SIZE>             Check repository integrity (missing batches/files)
///   --force                    Force regeneration of count file (with restart/unitary)
///   --output-path, -o        Optional: Directory for output files
///                            Defaults to current directory
///
/// Implementation:
///   - Hybrid approach: NoSetList (stack) for fast computation, NoSetListSerialized (heap) for compact I/O
///   - Creates .rkyv files with size_32 encoding (~2GB per 20M batch)
///   - 4-5× faster than heap-only v0.2.2 while maintaining compact file sizes

mod utils;
mod set;
mod no_set_list;
mod io_helpers;
mod filenames;
mod compaction;
mod list_of_nsl;
mod file_info;

use clap::Parser;
use separator::Separatable;
use crate::utils::*;

/// CLI arguments structure
#[derive(Parser, Debug)]
#[command(name = "funny_set_exploration")]
#[command(about = "Generate no-set lists for the Set card game",
    long_about = concat!(
        "Generate no-set lists for the Set card game\n\n",
        "MODES (examples and how common args affect each mode):\n\n",
        "1) Size mode (`--size`, `-s <SIZE|RANGE`)\n",
        "   - Purpose: Build one target size or a range (e.g. 5-7).\n",
        "   - Input path (-i): dir to read input files (required when\n",
        "     not using defaults).\n",
        "   - Output path (-o): dir to write outputs; if omitted,\n",
        "     defaults to input or current dir.\n",
        "   - --force: has no effect for `--size`.\n",
        "   - --keep_state: preserves partial/processed state files.\n",
        "   - Example: --size 5 -i ./in -o ./out\n\n",
        "2) Restart mode (`--restart <SIZE> <BATCH>`)\n",
        "   - Purpose: Resume from a specific input batch through\n",
        "     size 18.\n",
        "   - Input path (-i): source input directory for batches.\n",
        "   - Output path (-o): target output directory (defaults\n",
        "     to input if omitted).\n",
        "   - --force: regenerates the baseline count file by\n",
        "     scanning files before resuming.\n",
        "   - --keep_state: preserved if provided.\n",
        "   - Example: --restart 5 2 -i ./in -o ./out --force\n\n",
        "3) Unitary mode (`--unitary <SIZE> <BATCH>`)\n",
        "   - Purpose: Reprocess a single input batch to overwrite\n",
        "     or fix outputs.\n",
        "   - Input path (-i): dir containing the input batch.\n",
        "   - Output path (-o): where regenerated outputs are\n",
        "     written (defaults to input).\n",
        "   - --force: regenerates count baseline first.\n",
        "   - --keep_state: preserves state files for debugging.\n",
        "   - Example: --unitary 7 0 -i ./in --force\n\n",
        "4) Count mode (`--count <SIZE>`)\n",
        "   - Purpose: Count existing files for a size and create a\n",
        "     summary report.\n",
        "   - Input path (-i): dir to read files to count (required).\n",
        "   - Output path (-o): not used by this mode.\n",
        "   - --force: forces a full rescan/regeneration before\n",
        "     reporting.\n",
        "   - --keep_state: affects whether intermediary files are\n",
        "     preserved.\n",
        "   - Example: --count 6 -i ./out --force\n\n",
        "5) Check mode (`--check <SIZE>`)\n",
        "   - Purpose: Verify repository integrity for an output\n",
        "     size.\n",
        "   - Input path (-i): not used.\n",
        "   - Output path (-o): dir containing files to check\n",
        "     (defaults to current dir).\n",
        "   - --force/--keep_state: not applicable.\n",
        "   - Example: --check 8 -o ./out\n\n",
        "6) Compact mode (`--compact <SIZE>`)",
        "   - Purpose: Consolidate many small output files into\n",
        "     larger batches.\n",
        "   - Input path (-i): dir containing files to compact.\n",
        "   - Output path (-o): dir to write compacted files\n",
        "     (defaults to input).\n",
        "   - Example: --compact 12 -i ./out -o ./compacted\n\n",
        "7) Legacy-count mode (`--legacy-count <SIZE>` )\n",
        "   - Purpose: Read existing global/intermediary counts and\n",
        "     emit nsl_{size}_global_info.json/.txt without\n",
        "     recomputing intermediaries.\n",
        "   - Input path (-i): directory with count files (.txt).\n",
        "   - Output path: not used.\n\n",
        "COMMON FLAGS: -i/--input-path, -o/--output-path, --force,\n",
        "  --keep_state\n",
        "  The sections above show how each flag affects specific\n",
        "  modes (e.g. --force regenerates counts for --count,\n",
        "  --restart and --unitary).\n"
    )
)]
struct Args {
    /// Target output size: --size SIZE or --size SIZE BATCH
    /// Single argument: process from batch 0
    /// Two arguments: restart from specific input batch
    #[arg(short, long, num_args = 1..=2, value_names = ["SIZE", "BATCH"], conflicts_with_all = ["unitary"], help = "Target output size (optionally with start batch): SIZE [BATCH]")]
    size: Option<Vec<u32>>,

    /// Process a single input batch (unitary processing): <SIZE> <BATCH>
    /// Reprocesses exactly one input batch and regenerates outputs.
    #[arg(long, num_args = 2, value_names = ["SIZE", "BATCH"], conflicts_with_all = ["size", "count"], help = "Process a single input batch: SIZE BATCH")]
    unitary: Option<Vec<u32>>,

    /// Force regeneration of count file (affects --count, --size with batch, and --unitary)
    #[arg(long, help = "Force regeneration of count file (affects --count, --size with batch, and --unitary)")]
    force: bool,

    /// Keep partial and processed state files after a successful run
    #[arg(long, help = "Keep partial and processed state files after a run")]
    keep_state: bool,

    /// Count existing files for a specific size and create summary report
    #[arg(long, conflicts_with_all = ["size", "unitary", "compact", "legacy_count"], help = "Count files for a size and create a summary report")]
    count: Option<u8>,

    /// Legacy count: read existing global/intermediary counts and emit global info JSON/TXT
    #[arg(long, conflicts_with_all = ["size", "unitary", "count", "compact", "check"], help = "Legacy count: emit global info JSON/TXT from existing count files")]
    legacy_count: Option<u8>,

    /// Compact small output files into larger batches: <SIZE>
    /// Consolidates multiple small output files into larger batches.
    #[arg(long, conflicts_with_all = ["size", "restart", "unitary", "count", "check"], help = "Compact small files into larger batches for a target size")]
    compact: Option<u8>,

    /// Check repository integrity for a specific size
    /// Analyze files and count data for missing batches or files.
    #[arg(long, conflicts_with_all = ["size", "restart", "unitary", "count", "compact"], help = "Check repository integrity for a specific size")]
    check: Option<u8>,

    /// Input directory path (optional)
    /// Directory to read input files from; usage varies by mode.
    #[arg(short, long, help = "Input directory path (optional)")]
    input_path: Option<String>,

    /// Output directory path (optional)
    /// Directory to write output files to; usage varies by mode.
    #[arg(short, long, help = "Output directory path (optional)")]
    output_path: Option<String>,
}

/// Parse size argument into start and end range
/// Examples: "5" -> (5, 5), "5-7" -> (5, 7)


/// Unified configuration for all processing modes
#[derive(Debug)]
struct ProcessingConfig {
    mode: ProcessingMode,
    input_dir: String,
    output_dir: String,
    max_lists_per_file: u64,
    force_recount: bool,
    keep_state: bool,
}

/// Processing mode enumeration
#[derive(Debug)]
enum ProcessingMode {
    Count { size: u8 },
    LegacyCount { size: u8 },
    Check { size: u8 },
    Compact { size: u8 },
    Size { size: u8, start_batch: Option<u32> },
    Unitary { size: u8, batch: u32 },
    Default,
}

impl ProcessingMode {
    /// Check if this mode requires log file initialization
    fn requires_logging(&self) -> bool {
        matches!(self, 
            ProcessingMode::Count { .. } | 
            ProcessingMode::LegacyCount { .. } |
            ProcessingMode::Check { .. } | 
            ProcessingMode::Compact { .. })
    }
}

/// Validate size parameter for different modes
fn validate_size(size: u8, mode_name: &str, min: u8, max: u8) -> Result<(), String> {
    if size < min || size > max {
        Err(format!("Error: {} size {} out of range ({}-{})", mode_name, size, min, max))
    } else {
        Ok(())
    }
}

/// Resolve paths for modes that use both input and output with fallback logic
fn resolve_dual_path(input_arg: Option<&str>, output_arg: Option<&str>) -> (String, String) {
    match (input_arg, output_arg) {
        (Some(i), Some(o)) => (i.to_string(), o.to_string()),
        (Some(i), None) => (i.to_string(), i.to_string()),
        (None, Some(o)) => (o.to_string(), o.to_string()),
        (None, None) => (".".to_string(), ".".to_string()),
    }
}

/// Resolve input/output paths based on mode requirements
fn resolve_paths(
    mode: &ProcessingMode,
    input_arg: Option<&str>,
    output_arg: Option<&str>
) -> (String, String) {
    match mode {
        ProcessingMode::Count { .. } => {
            // Count only uses input
            (input_arg.unwrap_or(".").to_string(), String::new())
        },
        ProcessingMode::LegacyCount { .. } => {
            (input_arg.unwrap_or(".").to_string(), String::new())
        },
        ProcessingMode::Check { .. } => {
            // Check only uses output
            (String::new(), output_arg.unwrap_or(".").to_string())
        },
        ProcessingMode::Restart { .. } => {
            // Restart uses both, with fallback logic
            resolve_dual_path(input_arg, output_arg)
        },
        ProcessingMode::Unitary { .. } | ProcessingMode::SizeRange { .. } | ProcessingMode::Compact { .. } => {
            // These modes default output to input if not specified
            let input = input_arg.unwrap_or(".").to_string();
            let output = output_arg.unwrap_or(&input).to_string();
            (input, output)
        },
        ProcessingMode::Default => {
            // Default mode has hardcoded fallback
            let path = output_arg.unwrap_or(r"T:\data\funny_set_exploration").to_string();
            (path.clone(), path)
        }
    }
}

/// Handle force recount if enabled
fn handle_force_recount(
    enabled: bool,
    directory: &str,
    target_size: u8
    , keep_state: bool
) -> Result<(), String> {
    if !enabled {
        return Ok(());
    }
    
    use crate::list_of_nsl::count_size_files;
    
    test_print(&format!("\nFORCE MODE: Regenerating count file for size {}...", target_size));
    count_size_files(directory, target_size, true, keep_state)
        .map_err(|e| format!("Error regenerating count file: {}", e))?;
    test_print("Count file regenerated successfully\n");
    Ok(())
}

/// Print directories with consistent formatting
fn print_directories(input: &str, output: &str) {
    if !input.is_empty() {
        test_print(&format!("Input directory: {}", input));
    }
    if !output.is_empty() {
        test_print(&format!("Output directory: {}", output));
    }
}

/// Build unified configuration from parsed arguments
fn build_config(args: &Args, max_per_file: u64) -> Result<ProcessingConfig, String> {
    // Determine processing mode from arguments
    let mode = if let Some(compact_size) = args.compact {
        validate_size(compact_size, "Compact", 3, 18)?;
        ProcessingMode::Compact { size: compact_size }
    } else if let Some(legacy_size) = args.legacy_count {
        validate_size(legacy_size, "Legacy-count", 3, 18)?;
        ProcessingMode::LegacyCount { size: legacy_size }
    } else if let Some(check_size) = args.check {
        validate_size(check_size, "Check", 3, 18)?;
        ProcessingMode::Check { size: check_size }
    } else if let Some(count_size) = args.count {
        validate_size(count_size, "Count", 3, 18)?;
        ProcessingMode::Count { size: count_size }
    } else if let Some(ref size_vec) = args.size {
        let size = size_vec[0] as u8;
        validate_size(size, "Size", 3, 18)?;
        let start_batch = if size_vec.len() == 2 {
            let batch = size_vec[1];
            if size == 3 && batch > 0 {
                return Err("Cannot specify batch number for size 3 (seed lists)".to_string());
            }
            if size > 3 && batch == 0 {
                None // batch 0 is the default, treat as None
            } else if size > 3 {
                Some(batch)
            } else {
                None
            }
        } else {
            None
        };
        ProcessingMode::Size { size, start_batch }
    } else if let Some(ref unitary_vec) = args.unitary {
        if unitary_vec.len() != 2 {
            return Err("--unitary requires exactly 2 arguments: SIZE BATCH".to_string());
        }
        let size = unitary_vec[0] as u8;
        let batch = unitary_vec[1];
        validate_size(size, "Unitary", 3, 17)?;
        ProcessingMode::Unitary { size, batch }
    } else {
        ProcessingMode::Default
    };

    // Resolve paths based on mode
    // Compact mode must be in-place: disallow an explicit output path
    if let ProcessingMode::Compact { .. } = mode {
        if args.output_path.is_some() {
            return Err("Compact mode is in-place only; do not provide -o/--output-path".to_string());
        }
    }

    let (input_dir, output_dir) = resolve_paths(&mode, args.input_path.as_deref(), args.output_path.as_deref());

    Ok(ProcessingConfig {
        mode,
        input_dir,
        output_dir,
        max_lists_per_file: max_per_file,
        force_recount: args.force,
        keep_state: args.keep_state,
    })
}

/// Execute the appropriate mode based on configuration
fn execute_mode(config: &ProcessingConfig) -> Result<String, String> {
    use crate::list_of_nsl::{count_size_files, compact_size_files, check_size_files};
    use crate::file_info::{GlobalFileInfo};
    use std::path::Path;
    use std::fs;
    
    match &config.mode {
        ProcessingMode::Count { size } => {
            // Banner is printed by count_size_files function
            count_size_files(&config.input_dir, *size, config.force_recount, config.keep_state)
                .map_err(|e| format!("Error during count: {}", e))?;
            Ok("Count completed successfully".to_string())
        },

        ProcessingMode::LegacyCount { size } => {
            let base = &config.input_dir;
            let primary = Path::new(base).join(format!("nsl_{:02}_global_count.txt", size));
            let legacy_space = Path::new(base).join(format!("nsl_{:02}_global count.txt", size));
            let global_path = if primary.exists() {
                Some(primary)
            } else if legacy_space.exists() {
                Some(legacy_space)
            } else {
                None
            };

            let gfi_res = if let Some(path) = global_path {
                test_print(&format!("Reading existing global count {}", path.display()));
                GlobalFileInfo::from_global_count_file(&path)
            } else {
                test_print("Global count not found; reading intermediary files or scanning rkyv files...");
                GlobalFileInfo::from_intermediary_files(base, *size)
            };

            let mut gfi = gfi_res.map_err(|e| format!("Error loading counts: {}", e))?;
            let json_path = Path::new(base).join(format!("nsl_{:02}_global_info.json", size));
            let txt_path = Path::new(base).join(format!("nsl_{:02}_global_info.txt", size));

            gfi.save_json(&json_path).map_err(|e| format!("Error writing {}: {}", json_path.display(), e))?;
            let txt_body = gfi.to_txt(base, *size);
            fs::write(&txt_path, txt_body).map_err(|e| format!("Error writing {}: {}", txt_path.display(), e))?;

            test_print(&format!("Wrote {} and {}", json_path.display(), txt_path.display()));
            Ok("Legacy global info written".to_string())
        },
        
        ProcessingMode::Check { size } => {
            // Banner is printed by check_size_files function
            check_size_files(&config.output_dir, *size)
                .map_err(|e| format!("Error during check: {}", e))?;
            Ok("Check completed successfully".to_string())
        },
        
        ProcessingMode::Compact { size } => {
            // Banner is printed by compact_size_files function
            compact_size_files(&config.input_dir, &config.output_dir, *size, config.max_lists_per_file)
                .map_err(|e| format!("Error during compaction: {}", e))?;
            Ok("Compaction completed successfully".to_string())
        },
        
        ProcessingMode::Restart { size, batch } => {
            execute_restart_mode(config, *size, *batch)
        },
        
        ProcessingMode::Unitary { size, batch } => {
            execute_unitary_mode(config, *size, *batch)
        },
        
        ProcessingMode::SizeRange { start, end } => {
            execute_size_range_mode(config, *start, *end)
        },
        
        ProcessingMode::Default => {
            execute_default_mode(config)
        },
    }
}

/// Execute restart mode: resume from specific batch through size 18
fn execute_restart_mode(config: &ProcessingConfig, restart_size: u8, restart_batch: u32) -> Result<String, String> {
    use crate::list_of_nsl::ListOfNSL;
    use crate::file_info::GlobalFileState;
    
    test_print(&format!("RESTART MODE: Resuming from size {} batch {}", restart_size, restart_batch));
    test_print("Will process through size 18");
    test_print(&format!("Batch size: {} entries/file (~1GB, compact)", config.max_lists_per_file.separated_string()));
    print_directories(&config.input_dir, &config.output_dir);
    
    handle_force_recount(config.force_recount, &config.output_dir, restart_size + 1, config.keep_state)?;
    test_print("\n======================\n");

    let mut no_set_lists = ListOfNSL::with_paths(&config.input_dir, &config.output_dir);

    for target_size in (restart_size + 1)..=18 {
        let source_size = target_size - 1;
        let mut global_state = GlobalFileState::from_sources(&config.output_dir, target_size)
            .map_err(|e| format!("Failed to load global state: {}", e))?;
        
        if source_size == restart_size {
            test_print(&format!("Start processing files to create no-set-lists of size {} (from input batch {}):\n", 
                target_size, restart_batch));
            no_set_lists.process_from_batch(source_size, restart_batch, &config.max_lists_per_file, Some(&mut global_state));
        } else {
            test_print(&format!("Start processing files to create no-set-lists of size {}:\n", target_size));
            no_set_lists.process_all_files_of_current_size_n(source_size, &config.max_lists_per_file, Some(&mut global_state));
        }
        
        test_print(&format!("\nCompleted size {}!\n", target_size));
    }
    
    Ok(format!("Restart processing completed through size 18"))
}

/// Execute unitary mode: process a single input batch
fn execute_unitary_mode(config: &ProcessingConfig, unitary_size: u8, unitary_batch: u32) -> Result<String, String> {
    use crate::list_of_nsl::ListOfNSL;
    use crate::file_info::GlobalFileState;
    
    test_print(&format!("UNITARY MODE: Processing input size {} batch {}", unitary_size, unitary_batch));
    test_print(&format!("Output: size {} files", unitary_size + 1));
    test_print(&format!("Batch size: {} entries/file (~1GB, compact)", config.max_lists_per_file.separated_string()));
    print_directories(&config.input_dir, &config.output_dir);
    
    handle_force_recount(config.force_recount, &config.output_dir, unitary_size + 1, config.keep_state)?;
    test_print("\n======================\n");

    let mut no_set_lists = ListOfNSL::with_paths(&config.input_dir, &config.output_dir);
    let target_size = unitary_size + 1;
    let mut global_state = GlobalFileState::from_sources(&config.output_dir, target_size)
        .map_err(|e| format!("Failed to load global state: {}", e))?;
    
    test_print(&format!("Processing input size {} batch {}:", unitary_size, unitary_batch));
    no_set_lists.process_single_batch(unitary_size, unitary_batch, &config.max_lists_per_file, Some(&mut global_state));
    
    Ok(format!("Unitary processing completed for size {} batch {}", unitary_size, unitary_batch))
}

/// Execute size range mode: process one or more consecutive sizes
fn execute_size_range_mode(config: &ProcessingConfig, start_size: u8, end_size: u8) -> Result<String, String> {
    use crate::list_of_nsl::ListOfNSL;
    use crate::file_info::GlobalFileState;
    
    if start_size == end_size {
        test_print(&format!("Target size = {} cards", start_size));
    } else {
        test_print(&format!("Size range = {} to {} cards", start_size, end_size));
    }
    test_print(&format!("Batch size: {} entries/file (~1GB, compact)", config.max_lists_per_file.separated_string()));
    print_directories(&config.input_dir, &config.output_dir);
    test_print("\n======================\n");

    let mut no_set_lists = ListOfNSL::with_paths(&config.input_dir, &config.output_dir);

    // Handle size 3: create seed lists directly
    if start_size == 3 {
        test_print("Creating seed lists (size 3)...");
        no_set_lists.create_seed_lists();
        test_print("Seed lists created successfully.\n");
        
        // If only requesting size 3, we're done
        if end_size == 3 {
            return Ok("Seed lists (size 3) created successfully".to_string());
        }
    }

    // Handle size 4: need to create seed lists first if we haven't already
    if start_size == 4 {
        test_print("Creating seed lists (size 3)...");
        no_set_lists.create_seed_lists();
        test_print("Seed lists created successfully.\n");
    }

    // Process each size in the range (skip size 3 if it was the start)
    let process_start = if start_size == 3 { 4 } else { start_size };
    for target_size in process_start..=end_size {
        let source_size = target_size - 1;
        let mut global_state = GlobalFileState::from_sources(&config.output_dir, target_size)
            .map_err(|e| format!("Failed to load global state: {}", e))?;
        test_print(&format!("Start processing files to create no-set-lists of size {}:", target_size));
        
        no_set_lists.process_all_files_of_current_size_n(source_size, &config.max_lists_per_file, Some(&mut global_state));
        
        test_print(&format!("\nCompleted size {}! Generated files: no-set-list_{:02}_batch_*.rkyv\n", 
            target_size, target_size));
    }
    
    Ok(format!("Size range processing completed (sizes {} to {})", start_size, end_size))
}

/// Execute default mode: process the whole pipeline (seeds + sizes 4 to 18)
fn execute_default_mode(config: &ProcessingConfig) -> Result<String, String> {
    use crate::list_of_nsl::ListOfNSL;
    use crate::file_info::GlobalFileState;
    
    test_print("   - will create          58.896 no-set-lists with  3 cards");
    test_print("   - will create       1.004.589 no-set-lists with  4 cards");
    test_print("   - will create      13.394.538 no-set-lists with  5 cards");
    test_print("   - will create     141.370.218 no-set-lists with  6 cards");
    test_print("   - will create   1.180.345.041 no-set-lists with  7 cards");
    test_print("   - will create   7.920.450.378 no-set-lists with  8 cards");
    test_print("   - will create  43.126.538.805 no-set-lists with  9 cards");
    test_print("   - will create  __.___.___.___ no-set-lists with 10 cards");
    test_print("   - will create  __.___.___.___ no-set-lists with 11 cards");
    test_print("   - will create  __.___.___.___ no-set-lists with 12 cards");
    test_print("   - will create  __.___.___.___ no-set-lists with 13 cards");
    test_print("   - will create  __.___.___.___ no-set-lists with 14 cards");
    test_print("   - will create  __.___.___.___ no-set-lists with 15 cards");
    test_print("   - will create  __.___.___.___ no-set-lists with 16 cards");
    test_print("   - will create  __.___.___.___ no-set-lists with 17 cards");
    test_print("   - will create  __.___.___.___ no-set-lists with 18 cards");
    test_print("\n======================\n");

    let mut no_set_lists = ListOfNSL::with_path(&config.input_dir);

    // Create all seed lists
    test_print("Creating seed lists...");
    no_set_lists.create_seed_lists();

    // Expand from seed_lists to size 4, 5, 6...
    for size in 3..17 {
        let target_size = size + 1;
        let mut global_state = GlobalFileState::from_sources(&config.output_dir, target_size)
            .map_err(|e| format!("Failed to load global state: {}", e))?;
        test_print(&format!("\nStart processing files to create no-set-lists of size {}:", target_size));
        no_set_lists.process_all_files_of_current_size_n(size, &config.max_lists_per_file, Some(&mut global_state));
    }
    
    Ok("Default pipeline completed (sizes 3-18)".to_string())
}

fn main() {
    /// Max number of n-list saved per file for v0.4.0
    /// - Each NoSetList: 792 bytes during compute (stack)
    /// - Each NoSetListSerialized: ~100 bytes after conversion (heap)
    /// - 20M entries × 100 bytes = ~2GB per file after serialization
    /// - Peak RAM during save: ~10.5GB (vec + archive + overhead)
    const MAX_NLISTS_PER_FILE: u64 = 10_000_000;

    // Parse command-line arguments
    let args = Args::parse();

    // Setup debug/test printing
    debug_print_on();
    debug_print_off();
    test_print_off();
    test_print_on();

    // Build unified configuration
    let config = match build_config(&args, MAX_NLISTS_PER_FILE) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize logging for applicable modes
    if config.mode.requires_logging() {
        init_log_file();
    }

    banner(concat!("Funny Set Exploration [", env!("CARGO_PKG_VERSION"), "]"));
    
    // Execute mode and handle result
    match execute_mode(&config) {
        Ok(message) => {
            test_print(&format!("\n{}!", message));
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
