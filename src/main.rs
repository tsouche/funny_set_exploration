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
mod list_of_nsl;

use clap::Parser;
use separator::Separatable;
use crate::utils::*;

/// CLI arguments structure
#[derive(Parser, Debug)]
#[command(name = "funny_set_exploration")]
#[command(about = "Generate no-set lists for the Set card game", long_about = None)]
struct Args {
    /// Target size for the no-set lists (4-18 or range like 5-7)
    /// 
    /// If not provided, runs default behavior (creates seeds + sizes 4-18)
    /// - Single size: "5" builds size 5 from size 4 files
    /// - Range: "5-7" builds sizes 5, 6, and 7 sequentially
    /// - Size 4: Builds from seed lists (size 3)
    /// - Size 5+: Requires files from previous size
    #[arg(short, long, conflicts_with_all = ["restart", "unitary"])]
    size: Option<String>,

    /// Restart from specific input file: <SIZE> <BATCH>
    /// 
    /// SIZE refers to INPUT size. Processes from this batch onwards.
    /// 
    /// Examples:
    ///   --restart 5 2   Load size 5 batch 2, continue through size 18
    ///   --restart 7 0   Load size 7 batch 0, continue through size 18
    /// 
    /// By default, reads baseline from count file (no_set_list_count_XX.txt)
    /// Use --force to regenerate count file by scanning all files.
    #[arg(long, num_args = 2, value_names = ["SIZE", "BATCH"], conflicts_with_all = ["size", "count", "unitary"])]
    restart: Option<Vec<u32>>,

    /// Process a single input batch (unitary processing): <SIZE> <BATCH>
    /// 
    /// SIZE refers to INPUT size. Processes ONLY this specific batch.
    /// This is the ONLY canonical way to overwrite/fix defective files.
    /// Output files from this batch will be regenerated.
    /// 
    /// Examples:
    ///   --unitary 5 2   Reprocess size 5 batch 2 only (creates size 6)
    ///   --unitary 7 0   Reprocess size 7 batch 0 only (creates size 8)
    /// 
    /// Use --force to regenerate count file first (recalculates baseline).
    #[arg(long, num_args = 2, value_names = ["SIZE", "BATCH"], conflicts_with_all = ["size", "count", "restart"])]
    unitary: Option<Vec<u32>>,

    /// Force regeneration of count file when using --restart or --unitary
    /// 
    /// By default, restart/unitary modes read existing count file.
    /// This flag forces a full file scan to regenerate it first.
    #[arg(long)]
    force: bool,

    /// Count existing files for a specific size and create summary report
    /// 
    /// Examples:
    ///   --count 6   Count all size 6 files, create no_set_list_count_06.txt
    /// 
    /// Scans all files, counts lists, creates summary report
    /// without processing any new lists
    #[arg(long, conflicts_with_all = ["size", "restart", "unitary", "compact"])]
    count: Option<u8>,

    /// Compact small output files into larger batches: <SIZE>
    /// 
    /// SIZE refers to OUTPUT size to compact. Reads all files for this size,
    /// consolidates into 10M-entry batches, replaces original files.
    /// 
    /// New filename: nsl_compacted_{size:02}_batch_{batch:05}_from_{src:05}.rkyv
    /// 
    /// Examples:
    ///   --compact 8   Compact all size 8 files into 10M-entry batches
    ///   --compact 12  Compact all size 12 files into 10M-entry batches
    /// 
    /// Use when later processing creates many small files (ratio < 1.0).
    /// Original files are deleted after successful compaction.
    #[arg(long, conflicts_with_all = ["size", "restart", "unitary", "count", "check"])]
    compact: Option<u8>,

    /// Check repository integrity for a specific size
    /// 
    /// SIZE refers to OUTPUT size to check. Analyzes files and count data:
    /// - Lists missing output batches (should be continuous)
    /// - Lists files in intermediary count files but missing from directory
    /// 
    /// Examples:
    ///   --check 8   Check size 8 files for missing batches and files
    ///   --check 12  Check size 12 repository integrity
    /// 
    /// Requires count files to exist (run --count first).
    #[arg(long, conflicts_with_all = ["size", "restart", "unitary", "count", "compact"])]
    check: Option<u8>,

    /// Input directory path (optional)
    /// 
    /// Directory to read input files from.
    /// 
    /// Usage by mode:
    /// - count: Only uses -i (reads files to count)
    /// - check: Uses -o (repository to check)
    /// - size/range: Uses -i for input, -o for output (if only one, both)
    /// - unitary: Uses -i for input (writes to same dir unless -o given)
    /// - restart: Uses -i for input, -o for output (if only one, both)
    /// - compact: Uses -i for input files to compact
    /// 
    /// Examples:
    ///   Windows: T:\data\funny_set_exploration
    ///   Linux:   /mnt/nas/data/funny_set_exploration
    ///   Relative: ./input
    #[arg(short, long)]
    input_path: Option<String>,

    /// Output directory path (optional)
    /// 
    /// Directory to write output files to.
    /// 
    /// Usage by mode:
    /// - count: Not used
    /// - check: Uses for repository to check (default: current dir)
    /// - size/range: Uses for output (if omitted, uses -i or current dir)
    /// - unitary: Uses for output (if omitted, uses -i directory)
    /// - restart: Uses for output (if omitted, uses -i or current dir)
    /// - compact: Uses for compacted output files
    /// 
    /// Examples:
    ///   Windows: T:\data\funny_set_exploration
    ///   Linux:   /mnt/nas/data/funny_set_exploration
    ///   Relative: ./output
    #[arg(short, long)]
    output_path: Option<String>,
}

/// Parse size argument into start and end range
/// Examples: "5" -> (5, 5), "5-7" -> (5, 7)
fn parse_size_range(size_str: &str) -> Result<(u8, u8), String> {
    if size_str.contains('-') {
        let parts: Vec<&str> = size_str.split('-').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid range format: '{}'. Expected format: '5-7'", size_str));
        }
        let start: u8 = parts[0].trim().parse()
            .map_err(|_| format!("Invalid start size: '{}'", parts[0]))?;
        if start < 4 || start > 18 {
            return Err(format!("Start size {} out of range (4-18)", start));
        }
        let end: u8 = parts[1].trim().parse()
            .map_err(|_| format!("Invalid end size: '{}'", parts[1]))?;
        if end < 4 || end > 18 {
            return Err(format!("End size {} out of range (4-18)", end));
        }
        if start > end {
            return Err(format!("Start size {} cannot be greater than end size {}", start, end));
        }
        Ok((start, end))
    } else {
        let size: u8 = size_str.trim().parse()
            .map_err(|_| format!("Invalid size: '{}'", size_str))?;
        if size < 4 || size > 18 {
            return Err(format!("Size {} out of range (4-18)", size));
        }
        Ok((size, size))
    }
}

/// Unified configuration for all processing modes
#[derive(Debug)]
struct ProcessingConfig {
    mode: ProcessingMode,
    input_dir: String,
    output_dir: String,
    max_lists_per_file: u64,
    force_recount: bool,
}

/// Processing mode enumeration
#[derive(Debug)]
enum ProcessingMode {
    Count { size: u8 },
    Check { size: u8 },
    Compact { size: u8 },
    Restart { size: u8, batch: u32 },
    Unitary { size: u8, batch: u32 },
    SizeRange { start: u8, end: u8 },
    Default,
}

impl ProcessingMode {
    /// Check if this mode requires log file initialization
    fn requires_logging(&self) -> bool {
        matches!(self, 
            ProcessingMode::Count { .. } | 
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
) -> Result<(), String> {
    if !enabled {
        return Ok(());
    }
    
    use crate::list_of_nsl::count_size_files;
    
    test_print(&format!("\nFORCE MODE: Regenerating count file for size {}...", target_size));
    count_size_files(directory, target_size)
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
    } else if let Some(check_size) = args.check {
        validate_size(check_size, "Check", 3, 18)?;
        ProcessingMode::Check { size: check_size }
    } else if let Some(count_size) = args.count {
        validate_size(count_size, "Count", 3, 18)?;
        ProcessingMode::Count { size: count_size }
    } else if let Some(ref restart_vec) = args.restart {
        if restart_vec.len() != 2 {
            return Err("--restart requires exactly 2 arguments: SIZE BATCH".to_string());
        }
        let size = restart_vec[0] as u8;
        let batch = restart_vec[1];
        validate_size(size, "Restart", 4, 18)?;
        ProcessingMode::Restart { size, batch }
    } else if let Some(ref unitary_vec) = args.unitary {
        if unitary_vec.len() != 2 {
            return Err("--unitary requires exactly 2 arguments: SIZE BATCH".to_string());
        }
        let size = unitary_vec[0] as u8;
        let batch = unitary_vec[1];
        validate_size(size, "Unitary", 3, 17)?;
        ProcessingMode::Unitary { size, batch }
    } else if let Some(ref size_str) = args.size {
        let (start, end) = parse_size_range(size_str)?;
        ProcessingMode::SizeRange { start, end }
    } else {
        ProcessingMode::Default
    };

    // Resolve paths based on mode
    let (input_dir, output_dir) = resolve_paths(&mode, args.input_path.as_deref(), args.output_path.as_deref());

    Ok(ProcessingConfig {
        mode,
        input_dir,
        output_dir,
        max_lists_per_file: max_per_file,
        force_recount: args.force,
    })
}

/// Execute the appropriate mode based on configuration
fn execute_mode(config: &ProcessingConfig) -> Result<String, String> {
    use crate::list_of_nsl::{count_size_files, compact_size_files, check_size_files};
    
    match &config.mode {
        ProcessingMode::Count { size } => {
            // Banner is printed by count_size_files function
            count_size_files(&config.input_dir, *size)
                .map_err(|e| format!("Error during count: {}", e))?;
            Ok("Count completed successfully".to_string())
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
    
    test_print(&format!("RESTART MODE: Resuming from size {} batch {}", restart_size, restart_batch));
    test_print("Will process through size 18");
    test_print(&format!("Batch size: {} entries/file (~1GB, compact)", config.max_lists_per_file.separated_string()));
    print_directories(&config.input_dir, &config.output_dir);
    
    handle_force_recount(config.force_recount, &config.output_dir, restart_size + 1)?;
    test_print("\n======================\n");

    let mut no_set_lists = ListOfNSL::with_paths(&config.input_dir, &config.output_dir);

    for target_size in (restart_size + 1)..=18 {
        let source_size = target_size - 1;
        
        if source_size == restart_size {
            test_print(&format!("Start processing files to create no-set-lists of size {} (from input batch {}):\n", 
                target_size, restart_batch));
            no_set_lists.process_from_batch(source_size, restart_batch, &config.max_lists_per_file);
        } else {
            test_print(&format!("Start processing files to create no-set-lists of size {}:\n", target_size));
            no_set_lists.process_all_files_of_current_size_n(source_size, &config.max_lists_per_file);
        }
        
        test_print(&format!("\nCompleted size {}!\n", target_size));
    }
    
    Ok(format!("Restart processing completed through size 18"))
}

/// Execute unitary mode: process a single input batch
fn execute_unitary_mode(config: &ProcessingConfig, unitary_size: u8, unitary_batch: u32) -> Result<String, String> {
    use crate::list_of_nsl::ListOfNSL;
    
    test_print(&format!("UNITARY MODE: Processing input size {} batch {}", unitary_size, unitary_batch));
    test_print(&format!("Output: size {} files", unitary_size + 1));
    test_print(&format!("Batch size: {} entries/file (~1GB, compact)", config.max_lists_per_file.separated_string()));
    print_directories(&config.input_dir, &config.output_dir);
    
    handle_force_recount(config.force_recount, &config.output_dir, unitary_size + 1)?;
    test_print("\n======================\n");

    let mut no_set_lists = ListOfNSL::with_paths(&config.input_dir, &config.output_dir);
    
    test_print(&format!("Processing input size {} batch {}:", unitary_size, unitary_batch));
    no_set_lists.process_single_batch(unitary_size, unitary_batch, &config.max_lists_per_file);
    
    Ok(format!("Unitary processing completed for size {} batch {}", unitary_size, unitary_batch))
}

/// Execute size range mode: process one or more consecutive sizes
fn execute_size_range_mode(config: &ProcessingConfig, start_size: u8, end_size: u8) -> Result<String, String> {
    use crate::list_of_nsl::ListOfNSL;
    
    if start_size == end_size {
        test_print(&format!("Target size = {} cards", start_size));
    } else {
        test_print(&format!("Size range = {} to {} cards", start_size, end_size));
    }
    test_print(&format!("Batch size: {} entries/file (~1GB, compact)", config.max_lists_per_file.separated_string()));
    print_directories(&config.input_dir, &config.output_dir);
    test_print("\n======================\n");

    let mut no_set_lists = ListOfNSL::with_paths(&config.input_dir, &config.output_dir);

    // Handle size 4: need to create seed lists first
    if start_size == 4 {
        test_print("Creating seed lists (size 3)...");
        no_set_lists.create_seed_lists();
        test_print("Seed lists created successfully.\n");
    }

    // Process each size in the range
    for target_size in start_size..=end_size {
        let source_size = target_size - 1;
        test_print(&format!("Start processing files to create no-set-lists of size {}:", target_size));
        
        no_set_lists.process_all_files_of_current_size_n(source_size, &config.max_lists_per_file);
        
        test_print(&format!("\nCompleted size {}! Generated files: no-set-list_{:02}_batch_*.rkyv\n", 
            target_size, target_size));
    }
    
    Ok(format!("Size range processing completed (sizes {} to {})", start_size, end_size))
}

/// Execute default mode: process the whole pipeline (seeds + sizes 4 to 18)
fn execute_default_mode(config: &ProcessingConfig) -> Result<String, String> {
    use crate::list_of_nsl::ListOfNSL;
    
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
        test_print(&format!("\nStart processing files to create no-set-lists of size {}:", size + 1));
        no_set_lists.process_all_files_of_current_size_n(size, &config.max_lists_per_file);
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

    banner("Funny Set Exploration)");
    
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
