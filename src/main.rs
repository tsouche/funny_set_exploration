/// Manage the search for the grail of Set: combinations of up to 20 cards 
/// with no sets
///
/// Version 0.4.14 - Added --save-history mode for historical state preservation
/// Added: --save-history mode to merge current state with historical records
/// Enhanced: Automatic history saving after --size, --unitary, and --cascade modes
/// Enhanced: Cascade mode calls internal functions instead of spawning subprocesses
/// Previous: --cascade mode for automated multi-size processing
/// Previous: --size mode with compaction workflow for sizes 13+
/// Previous: Automatic input/output compaction for sizes 13+
/// 
/// CLI Usage:
///   funny.exe --size 3 -o .\output                          # Create seed lists (size 3)
///   funny.exe --size 5 -i .\input -o .\output               # Build size 5 from size 4
///   funny.exe --size 5 2 -i .\input -o .\output             # Restart size 5 from input batch 2
///   funny.exe --size 14 -i .\input -o .\output              # Build size 14 (auto-compact input & output)
///   funny.exe --size 14 -i .\input -o .\output --force      # Build size 14 (process all files, not just compacted)
///   funny.exe --unitary 5 2 -i .\input -o .\output          # Process only input batch 2
///   funny.exe --cascade 12 -i X:\funny                      # Cascade from size 12 (process 13-20)
///   funny.exe --save-history 14 -i .\14_to_15               # Save historical state for size 14
///   funny.exe --count 6 -i .\output                         # Count size 6 files
///   funny.exe --check 6 -o .\output                         # Check size 6 integrity
///   funny.exe --compact 15 -i .\14_to_15                    # Compact all size 15 files
///   funny.exe --compact 15 5000 -i .\14_to_15               # Compact up to batch 5000
///   funny.exe                                               # Default mode (sizes 4-20)
///
/// Arguments:
///   --size, -s <SIZE> [BATCH]  Target output size (3-20), optional batch to restart from
///                              If omitted, runs default behavior (creates seeds + sizes 4-20)
///   --unitary <SIZE> <BATCH>   Process only one specific input batch (unitary processing)
///   --cascade <INPUT_SIZE>     Process all sizes from INPUT_SIZE (12-19) to size 20
///                              Automatically detects last processed batch per size
///   --save-history <SIZE>      Merge current state with historical records for preservation
///                              Automatically called after --size, --unitary, --cascade
///   --count <SIZE>             Count existing files and create summary report
///   --check <SIZE>             Check repository integrity (missing batches/files)
///   --force                    Force regeneration of count file (with size batch/unitary)
///   --input-path, -i           Optional: Directory for input files (defaults to current)
///                              For cascade mode: root directory with subdirectories
///   --output-path, -o          Optional: Directory for output files (defaults to input)
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
        "1) Size mode (`--size`, `-s <SIZE> [BATCH]`)\n",
        "   - Purpose: Build a specific output size.\n",
        "   - Single arg (--size 5): Process size 5 from input batch 0.\n",
        "   - Two args (--size 5 2): Resume size 5 from input batch 2.\n",
        "   - Input path (-i): dir to read input files (defaults to\n",
        "     current dir).\n",
        "   - Output path (-o): dir to write outputs (defaults to\n",
        "     input dir).\n",
        "   - --force: regenerates count file when restarting from\n",
        "     a batch.\n",
        "   - --keep_state: preserves partial/processed state files.\n",
        "   - Example: --size 5 -i ./in -o ./out\n",
        "   - Example: --size 5 2 -i ./in -o ./out --force\n\n",
        "2) Unitary mode (`--unitary <SIZE> <BATCH>`)\n",
        "   - Purpose: Reprocess a single input batch to overwrite\n",
        "     or fix outputs.\n",
        "   - Input path (-i): dir containing the input batch.\n",
        "   - Output path (-o): where regenerated outputs are\n",
        "     written (defaults to input).\n",
        "   - --force: regenerates count baseline first.\n",
        "   - --keep_state: preserves state files for debugging.\n",
        "   - Example: --unitary 7 0 -i ./in --force\n\n",
        "3) Count mode (`--count <SIZE>`)\n",
        "   - Purpose: Count existing files for a size and create a\n",
        "     summary report.\n",
        "   - Input path (-i): dir to read files to count (required).\n",
        "   - Output path (-o): not used by this mode.\n",
        "   - --force: forces a full rescan/regeneration before\n",
        "     reporting.\n",
        "   - --keep_state: affects whether intermediary files are\n",
        "     preserved.\n",
        "   - Example: --count 6 -i ./out --force\n\n",
        "4) Check mode (`--check <SIZE>`)\n",
        "   - Purpose: Verify repository integrity for an output\n",
        "     size.\n",
        "   - Input path (-i): not used.\n",
        "   - Output path (-o): dir containing files to check\n",
        "     (defaults to current dir).\n",
        "   - --force/--keep_state: not applicable.\n",
        "   - Example: --check 8 -o ./out\n\n",
        "5) Compact mode (`--compact <SIZE> [MAX_BATCH]`)\\n",
        "   - Purpose: Consolidate many small output files into\\n",
        "     larger batches.\\n",
        "   - Optional MAX_BATCH: stop compaction after processing\\n",
        "     files up to this output batch number.\\n",
        "   - Input path (-i): dir containing files to compact.\\n",
        "   - Output path (-o): dir to write compacted files\\n",
        "     (defaults to input).\\n",
        "   - Example: --compact 12 -i ./out\\n",
        "   - Example: --compact 12 5000 -i ./out (stop at batch 5000)\\n\\n",
        "6) Legacy-count mode (`--legacy-count <SIZE>` )\n",
        "   - Purpose: Read existing global/intermediary counts and\n",
        "     emit nsl_{size}_global_info.json/.txt without\n",
        "     recomputing intermediaries.\n",
        "   - Input path (-i): directory with count files (.txt).\n",
        "   - Output path: not used.\n\n",
        "7) Create-JSON mode (`--create-json <SIZE>`)\n",
        "   - Purpose: Export human-readable JSON and TXT files from\n",
        "     the rkyv state file (write-only, for inspection).\n",
        "   - Input path (-i): directory with rkyv state file.\n",
        "   - Output path: not used.\n",
        "   - Example: --create-json 10 -i ./09_to_10\n\n",
        "8) Cascade mode (`--cascade <INPUT_SIZE>`)\n",
        "   - Purpose: Process all output sizes starting from a given\n",
        "     input size (12-19) up to size 20.\n",
        "   - Automatically detects last processed batch per size and\n",
        "     continues from there.\n",
        "   - Input path (-i): root directory containing subdirectories\n",
        "     (11_to_12, 12_to_13c, 13c_to_14c, etc.).\n",
        "   - Output path: not used (determined automatically).\n",
        "   - Example: --cascade 12 -i X:\\funny\n",
        "   - Directory structure expected:\n",
        "     11_to_12/         (input for size 13)\n",
        "     12_to_13c/        (output size 13, input for 14)\n",
        "     13c_to_14c/       (output size 14, input for 15)\n",
        "     ... and so on\n\n",
        "COMMON FLAGS: -i/--input-path, -o/--output-path, --force,\n",
        "  --keep_state\n",
        "  The sections above show how each flag affects specific\n",
        "  modes (e.g. --force regenerates counts for --count,\n",
        "  --size with batch, and --unitary).\n"
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
    
    /// Create human-readable JSON/TXT exports from rkyv state file
    #[arg(long, conflicts_with_all = ["size", "unitary", "count", "compact", "check", "legacy_count"], help = "Export JSON and TXT files from rkyv state (human-readable format)")]
    create_json: Option<u8>,

    /// Compact small output files into larger batches: <SIZE> [MAX_BATCH]
    /// Consolidates multiple small output files into larger batches.
    /// Optional MAX_BATCH parameter stops compaction after processing files up to that batch number.
    #[arg(long, num_args = 1..=2, value_names = ["SIZE", "MAX_BATCH"], conflicts_with_all = ["size", "unitary", "count", "check"], help = "Compact small files into larger batches for a target size, optionally up to MAX_BATCH")]
    compact: Option<Vec<u32>>,

    /// Check repository integrity for a specific size
    /// Analyze files and count data for missing batches or files.
    #[arg(long, conflicts_with_all = ["size", "unitary", "count", "compact"], help = "Check repository integrity for a specific size")]
    check: Option<u8>,

    /// Cascade mode: process all sizes starting from a given input size
    /// Generates output files of growing sizes by processing unprocessed batches.
    /// Takes the starting input size (12-19) and uses the current directory or -i as root.
    #[arg(long, conflicts_with_all = ["size", "unitary", "count", "compact", "check"], help = "Cascade mode: process sizes starting from input size (12-19)")]
    cascade: Option<u8>,

    /// Save history mode: merge current state with historical state
    /// Preserves records of all files ever processed, even if deleted.
    #[arg(long, conflicts_with_all = ["size", "unitary", "count", "compact", "check", "cascade"], help = "Save history: merge current state with historical records for a size")]
    save_history: Option<u8>,

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
    CreateJson { size: u8 },
    Check { size: u8 },
    Compact { size: u8, max_batch: Option<u32> },
    Size { size: u8, start_batch: Option<u32> },
    Unitary { size: u8, batch: u32 },
    Cascade { starting_input_size: u8, root_directory: String },
    SaveHistory { size: u8 },
    Default,
}

impl ProcessingMode {
    /// Check if this mode requires log file initialization
    fn requires_logging(&self) -> bool {
        matches!(self, 
            ProcessingMode::Count { .. } | 
            ProcessingMode::LegacyCount { .. } |
            ProcessingMode::CreateJson { .. } |
            ProcessingMode::Check { .. } | 
            ProcessingMode::Compact { .. } |
            ProcessingMode::Cascade { .. } |
            ProcessingMode::SaveHistory { .. })
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
        ProcessingMode::CreateJson { .. } => {
            (input_arg.unwrap_or(".").to_string(), String::new())
        },
        ProcessingMode::Check { .. } => {
            // Check only uses output
            (String::new(), output_arg.unwrap_or(".").to_string())
        },
        ProcessingMode::Cascade { .. } => {
            // Cascade uses input as root directory
            let root = input_arg.unwrap_or(".").to_string();
            (root, String::new())
        },
        ProcessingMode::SaveHistory { .. } => {
            // SaveHistory uses input directory
            (input_arg.unwrap_or(".").to_string(), String::new())
        },
        ProcessingMode::Size { .. } | ProcessingMode::Unitary { .. } | ProcessingMode::Compact { .. } => {
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
    let mode = if let Some(starting_input_size) = args.cascade {
        validate_size(starting_input_size, "Cascade", 12, 19)?;
        let root_directory = args.input_path.clone().unwrap_or_else(|| ".".to_string());
        ProcessingMode::Cascade { starting_input_size, root_directory }
    } else if let Some(save_history_size) = args.save_history {
        validate_size(save_history_size, "SaveHistory", 3, 20)?;
        ProcessingMode::SaveHistory { size: save_history_size }
    } else if let Some(ref compact_vec) = args.compact {
        let compact_size = compact_vec[0] as u8;
        validate_size(compact_size, "Compact", 3, 20)?;
        let max_batch = if compact_vec.len() == 2 {
            Some(compact_vec[1])
        } else {
            None
        };
        ProcessingMode::Compact { size: compact_size, max_batch }
    } else if let Some(legacy_size) = args.legacy_count {
        validate_size(legacy_size, "Legacy-count", 3, 20)?;
        ProcessingMode::LegacyCount { size: legacy_size }
    } else if let Some(create_json_size) = args.create_json {
        validate_size(create_json_size, "Create-json", 3, 20)?;
        ProcessingMode::CreateJson { size: create_json_size }
    } else if let Some(check_size) = args.check {
        validate_size(check_size, "Check", 3, 20)?;
        ProcessingMode::Check { size: check_size }
    } else if let Some(count_size) = args.count {
        validate_size(count_size, "Count", 3, 20)?;
        ProcessingMode::Count { size: count_size }
    } else if let Some(ref size_vec) = args.size {
        let size = size_vec[0] as u8;
        validate_size(size, "Size", 3, 20)?;
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
        validate_size(size, "Unitary", 3, 19)?;
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
            use crate::file_info::GlobalFileState;
            use std::collections::HashSet;
            use std::io::BufRead;
            
            let input_base = &config.input_dir;
            test_print(&format!("Legacy-count mode for size {:02}", size));
            
            // Step 1: Load from JSON first (authoritative format if available)
            let mut state = GlobalFileState::from_sources(input_base, *size)
                .unwrap_or_else(|_| {
                    test_print("   ... No existing state found, starting fresh");
                    GlobalFileState::new(input_base, *size)
                });
            
            let initial_count = state.entries().len();
            let mut seen_files: HashSet<String> = state.entries().keys()
                .map(|(_, _, filename)| filename.clone())
                .collect();
            let mut processed_batches: HashSet<u32> = state.entries().values()
                .map(|e| e.source_batch)
                .collect();
            
            test_print(&format!("   ... Loaded {} files from {} source batches", 
                initial_count, processed_batches.len()));
            
            // Step 2: Complement with intermediary count files
            let mut files_added = 0;
            let mut added_from_rkyv = 0;
            let pattern = format!("nsl_{:02}_intermediate_count_from_{:02}_", size, size - 1);
            let mut intermediary_files: Vec<(std::path::PathBuf, u32)> = Vec::new();
            
            for entry in fs::read_dir(input_base).map_err(|e| format!("Error reading directory: {}", e))? {
                if let Ok(e) = entry {
                    if let Some(name) = e.file_name().to_str() {
                        if name.starts_with(&pattern) && name.ends_with(".txt") {
                            if let Some(batch_str) = name.rsplit('_').next().and_then(|s| s.strip_suffix(".txt")) {
                                if let Ok(batch) = batch_str.parse::<u32>() {
                                    intermediary_files.push((e.path(), batch));
                                }
                            }
                        }
                    }
                }
            }
            
            intermediary_files.sort_by_key(|(_, batch)| *batch);
            let unprocessed: Vec<_> = intermediary_files.iter()
                .filter(|(_, batch)| !processed_batches.contains(batch))
                .collect();
            
            if !unprocessed.is_empty() {
                test_print(&format!("   ... Found {} unprocessed intermediate count files", unprocessed.len()));
                
                for (path, batch) in unprocessed {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        let file = fs::File::open(path).map_err(|e| format!("Error opening {}: {}", name, e))?;
                        let reader = std::io::BufReader::new(file);
                        
                        for line in reader.lines() {
                            let line = line.map_err(|e| format!("Error reading line: {}", e))?;
                            // Strip UTF-8 BOM if present
                            let line_clean = line.strip_prefix('\u{FEFF}').unwrap_or(&line);
                            let trimmed = line_clean.trim();
                            
                            if trimmed.starts_with("...") {
                                // Parse: "...  8528436 lists in filename.rkyv"
                                if let Some(rest) = trimmed.strip_prefix("...") {
                                    let rest = rest.trim();
                                    let parts: Vec<&str> = rest.split_whitespace().collect();
                                    if parts.len() >= 4 && parts[1] == "lists" && parts[2] == "in" {
                                        if let Ok(count) = parts[0].parse::<u64>() {
                                            let filename = parts[3].to_string();
                                            
                                            if seen_files.contains(&filename) {
                                                continue;
                                            }
                                            
                                            // Parse batch numbers from filename
                                            if let Some(to_pos) = filename.find("_to_") {
                                                let before_to = &filename[..to_pos];
                                                let after_raw = &filename[to_pos + 4..];
                                                let after_to = after_raw
                                                    .strip_suffix("_compacted.rkyv")
                                                    .or_else(|| after_raw.strip_suffix(".rkyv"))
                                                    .unwrap_or(after_raw);
                                                
                                                if let Some(src_pos) = before_to.rfind("_batch_") {
                                                    if let Ok(src_batch) = before_to[src_pos + 7..].parse::<u32>() {
                                                        if let Some(tgt_pos) = after_to.rfind("_batch_") {
                                                            if let Ok(tgt_batch) = after_to[tgt_pos + 7..].parse::<u32>() {
                                                                let is_compacted = filename.contains("_compacted.rkyv");
                                                                state.register_file(&filename, src_batch, tgt_batch, count, is_compacted, None, None);
                                                                seen_files.insert(filename);

                                                                files_added += 1;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        processed_batches.insert(*batch);
                    }
                }
                
                test_print(&format!("   ... Added {} new files from intermediate counts", files_added));
            }
            
            // Step 3: If --force, scan rkyv files directly to fill remaining gaps
            if config.force_recount {
                test_print("   ... FORCE mode: Scanning .rkyv files to fill gaps...");
                
                let mut rkyv_files: Vec<std::path::PathBuf> = Vec::new();
                for entry in fs::read_dir(input_base).map_err(|e| format!("Error reading directory: {}", e))? {
                    if let Ok(e) = entry {
                        if let Some(name) = e.file_name().to_str() {
                            if name.ends_with(".rkyv") && name.contains(&format!("_to_{:02}_", size)) {
                                rkyv_files.push(e.path());
                            }
                        }
                    }
                }
                
                test_print(&format!("   ... Found {} total rkyv files in directory", rkyv_files.len()));
                
                // Filter to only files not already in state
                let missing_files: Vec<_> = rkyv_files.iter()
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|name| !seen_files.contains(name))
                            .unwrap_or(false)
                    })
                    .collect();
                
                test_print(&format!("   ... {} files missing from state, need introspection", missing_files.len()));
                
                if missing_files.is_empty() {
                    test_print("   ... All rkyv files already in state, nothing to introspect");
                } else {
                    let total_missing = missing_files.len();
                    let mut processed = 0;
                    for path in missing_files {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            processed += 1;
                            test_print(&format!("   ... [{}/{}] Reading {}", processed, total_missing, name));
                            
                            // Parse batch numbers
                            if let Some(to_pos) = name.find("_to_") {
                                let before_to = &name[..to_pos];
                                let after_raw = &name[to_pos + 4..];
                                let after_to = after_raw
                                    .strip_suffix("_compacted.rkyv")
                                    .or_else(|| after_raw.strip_suffix(".rkyv"))
                                    .unwrap_or(after_raw);
                                
                                if let Some(src_pos) = before_to.rfind("_batch_") {
                                    if let Ok(src_batch) = before_to[src_pos + 7..].parse::<u32>() {
                                        if let Some(tgt_pos) = after_to.rfind("_batch_") {
                                            if let Ok(tgt_batch) = after_to[tgt_pos + 7..].parse::<u32>() {
                                                // Count lists in rkyv file
                                                use memmap2::Mmap;
                                                use rkyv::check_archived_root;
                                                use crate::no_set_list::NoSetListSerialized;
                                                
                                                if let Ok(file) = fs::File::open(&path) {
                                                    if let Ok(mmap) = unsafe { Mmap::map(&file) } {
                                                        if let Ok(arch) = check_archived_root::<Vec<NoSetListSerialized>>(&mmap[..]) {
                                                            let count = arch.len() as u64;
                                                            let is_compacted = name.contains("_compacted.rkyv");
                                                            
                                                            // Get file metadata
                                                            let (file_size, mtime) = path.metadata()
                                                                .ok()
                                                                .map(|m| (
                                                                    Some(m.len()),
                                                                    m.modified().ok()
                                                                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                                                        .map(|d| d.as_secs() as i64)
                                                                ))
                                                                .unwrap_or((None, None));
                                                            
                                                            state.register_file(name, src_batch, tgt_batch, count, is_compacted, file_size, mtime);
                                                            seen_files.insert(name.to_string());
                                                            added_from_rkyv += 1;
                                                            
                                                            test_print(&format!("       {} lists counted, saving state...", count));
                                                            state.flush().map_err(|e| format!("Error saving rkyv after {}: {}", name, e))?;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                if added_from_rkyv > 0 {
                    test_print(&format!("   ... Added {} files from direct rkyv scan", added_from_rkyv));
                }
            }
            
            // Only save if we actually added new data
            let total_files_added = files_added + added_from_rkyv;
            
            if total_files_added > 0 {
                test_print("   ... Saving updated state...");
                state.flush().map_err(|e| format!("Error saving rkyv: {}", e))?;
                state.export_human_readable().map_err(|e| format!("Error exporting JSON/TXT: {}", e))?;
                
                let rkyv_path = Path::new(input_base).join(format!("nsl_{:02}_global_info.rkyv", size));
                let json_path = Path::new(input_base).join(format!("nsl_{:02}_global_info.json", size));
                let txt_path = Path::new(input_base).join(format!("nsl_{:02}_global_info.txt", size));
                
                test_print(&format!("Wrote {}, {} and {}", rkyv_path.display(), json_path.display(), txt_path.display()));
            } else {
                test_print("   ... No changes detected, skipping file writes");
            }
            
            test_print(&format!("Total: {} files from {} unique source batches", 
                state.entries().len(), 
                state.entries().values().map(|e| e.source_batch).collect::<HashSet<_>>().len()));
            Ok("Legacy count completed successfully".to_string())
        },
        
        ProcessingMode::CreateJson { size } => {
            use crate::file_info::GlobalFileState;
            
            test_print(&format!("Creating human-readable JSON/TXT exports for size {:02}...", size));
            
            // Load state from rkyv (authoritative format)
            let state = GlobalFileState::from_sources(&config.input_dir, *size)
                .map_err(|e| format!("Error loading state: {}", e))?;
            
            test_print(&format!("   ... Loaded {} files from rkyv state", state.entries().len()));
            
            // Export to human-readable formats
            state.export_human_readable()
                .map_err(|e| format!("Error exporting JSON/TXT: {}", e))?;
            
            let json_path = Path::new(&config.input_dir).join(format!("nsl_{:02}_global_info.json", size));
            let txt_path = Path::new(&config.input_dir).join(format!("nsl_{:02}_global_info.txt", size));
            
            test_print(&format!("Exported {} and {}", json_path.display(), txt_path.display()));
            Ok("JSON/TXT export completed successfully".to_string())
        },
        
        ProcessingMode::Check { size } => {
            // Banner is printed by check_size_files function
            check_size_files(&config.output_dir, *size)
                .map_err(|e| format!("Error during check: {}", e))?;
            Ok("Check completed successfully".to_string())
        },
        
        ProcessingMode::Compact { size, max_batch } => {
            // Banner is printed by compact_size_files function
            compact_size_files(&config.input_dir, &config.output_dir, *size, config.max_lists_per_file, *max_batch)
                .map_err(|e| format!("Error during compaction: {}", e))?;
            Ok("Compaction completed successfully".to_string())
        },
        
        ProcessingMode::Size { size, start_batch } => {
            execute_size_mode(config, *size, *start_batch)
        },
        
        ProcessingMode::Unitary { size, batch } => {
            execute_unitary_mode(config, *size, *batch)
        },
        
        ProcessingMode::Cascade { starting_input_size, root_directory } => {
            execute_cascade_mode(*starting_input_size, root_directory, config.max_lists_per_file)
        },
        
        ProcessingMode::SaveHistory { size } => {
            execute_save_history_mode(&config.input_dir, *size)
        },
        
        ProcessingMode::Default => {
            execute_default_mode(config)
        },
    }
}

/// Execute size mode: process specific size, optionally restarting from a batch
fn execute_size_mode(config: &ProcessingConfig, output_size: u8, start_batch: Option<u32>) -> Result<String, String> {
    use crate::list_of_nsl::ListOfNSL;
    use crate::file_info::GlobalFileState;
    use crate::filenames::get_last_compacted_batch;
    use crate::compaction::compact_size_files;
    
    if let Some(batch) = start_batch {
        test_print(&format!("RESTART MODE: Resuming output size {} from input batch {}", output_size, batch));
        handle_force_recount(config.force_recount, &config.output_dir, output_size, config.keep_state)?;
    } else {
        test_print(&format!("Target output size = {} cards", output_size));
    }
    test_print(&format!("Batch size: {} entries/file (~1GB, compact)", config.max_lists_per_file.separated_string()));
    print_directories(&config.input_dir, &config.output_dir);
    test_print("\n======================\n");

    let mut no_set_lists = ListOfNSL::with_paths(&config.input_dir, &config.output_dir);

    // Handle size 3: create seed lists directly
    if output_size == 3 {
        test_print("Creating seed lists (size 3)...");
        no_set_lists.create_seed_lists();
        test_print("Seed lists created successfully.\n");
        return Ok("Seed lists (size 3) created successfully".to_string());
    }

    // For size 4+, need to create seed lists first if starting from batch 0
    if output_size == 4 && start_batch.is_none() {
        test_print("Creating seed lists (size 3)...");
        // Create seed lists with output to input directory (so they don't pollute output dir)
        let mut seed_generator = ListOfNSL::with_paths(&config.input_dir, &config.input_dir);
        seed_generator.create_seed_lists();
        test_print("Seed lists created successfully.\n");
    }

    // Step 1: For sizes 13+, run compaction on input directory before processing
    let source_size = output_size - 1;
    if source_size >= 13 {
        test_print(&format!("\n=== Pre-processing: Compacting input files (size {}) ===", source_size));
        match compact_size_files(&config.input_dir, &config.input_dir, source_size, config.max_lists_per_file, None) {
            Ok(_) => test_print("Input compaction completed successfully.\n"),
            Err(e) => test_print(&format!("Warning: Input compaction encountered an issue: {}\n", e)),
        }
    }

    // Step 2: Determine processing range
    // If --force is not set and source size >= 13, only process up to the last compacted batch
    let max_input_batch = if !config.force_recount && source_size >= 13 {
        match get_last_compacted_batch(&config.input_dir, source_size) {
            Some(last_compacted) => {
                test_print(&format!("Processing only compacted input files up to batch {:06} (use --force to process all files)", last_compacted));
                Some(last_compacted)
            }
            None => {
                test_print("Warning: No compacted input files found. Will process all available files.");
                None
            }
        }
    } else {
        None
    };

    // Step 3: Process the requested size
    let mut global_state = GlobalFileState::from_sources(&config.output_dir, output_size)
        .map_err(|e| format!("Failed to load global state: {}", e))?;
    
    if let Some(batch) = start_batch {
        test_print(&format!("Start processing from input batch {} to create no-set-lists of size {}:", batch, output_size));
        
        // If max_input_batch is set, we need to handle the range specially
        if let Some(max_batch) = max_input_batch {
            if batch <= max_batch {
                // Process from start_batch up to max_batch
                test_print(&format!("   ... processing batches {:06} to {:06} (compacted only)", batch, max_batch));
                no_set_lists.process_batch_range(source_size, batch, max_batch, &config.max_lists_per_file, Some(&mut global_state));
            } else {
                test_print(&format!("Warning: Start batch {} is beyond last compacted batch {}. No processing needed.", batch, max_batch));
            }
        } else {
            no_set_lists.process_from_batch(source_size, batch, &config.max_lists_per_file, Some(&mut global_state));
        }
    } else {
        test_print(&format!("Start processing files to create no-set-lists of size {}:", output_size));
        
        if let Some(max_batch) = max_input_batch {
            // Process from 0 to max_batch
            test_print(&format!("   ... processing batches 000000 to {:06} (compacted only)", max_batch));
            no_set_lists.process_batch_range(source_size, 0, max_batch, &config.max_lists_per_file, Some(&mut global_state));
        } else {
            no_set_lists.process_all_files_of_current_size_n(source_size, &config.max_lists_per_file, Some(&mut global_state));
        }
    }
    
    test_print(&format!("\nCompleted size {}! Generated files: no-set-list_{:02}_batch_*.rkyv\n", output_size, output_size));
    
    // Step 4: For sizes 13+, run compaction on output directory after processing
    if output_size >= 13 {
        test_print(&format!("\n=== Post-processing: Compacting output files (size {}) ===", output_size));
        match compact_size_files(&config.output_dir, &config.output_dir, output_size, config.max_lists_per_file, None) {
            Ok(_) => {
                test_print("Output compaction completed successfully.\n");
                // Note: compact_size_files already exports human-readable files (JSON/TXT)
            },
            Err(e) => test_print(&format!("Warning: Output compaction encountered an issue: {}\n", e)),
        }
    } else {
        // For sizes < 13, no compaction runs, so we need to export human-readable files here
        test_print(&format!("\nExporting global state files for size {}...", output_size));
        match global_state.export_human_readable() {
            Ok(_) => test_print(&format!("Exported: {}/nsl_{:02}_global_info.json and .txt\n", config.output_dir, output_size)),
            Err(e) => test_print(&format!("Warning: Failed to export JSON/TXT: {}\n", e)),
        }
    }
    
    // Save history at the end
    test_print(&format!("\nSaving historical state for size {}...", output_size));
    let history_config = ProcessingConfig {
        mode: ProcessingMode::SaveHistory { size: output_size },
        input_dir: config.output_dir.clone(),
        output_dir: String::new(),
        max_lists_per_file: config.max_lists_per_file,
        force_recount: false,
        keep_state: false,
    };
    match execute_mode(&history_config) {
        Ok(_) => test_print("Historical state saved successfully.\n"),
        Err(e) => test_print(&format!("Warning: Failed to save history: {}\n", e)),
    }
    
    if start_batch.is_some() {
        Ok(format!("Size {} processing completed (restarted from batch {})", output_size, start_batch.unwrap()))
    } else {
        Ok(format!("Size {} processing completed", output_size))
    }
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
    
    // Export human-readable state files
    test_print(&format!("\nExporting global state files for size {}...", target_size));
    match global_state.export_human_readable() {
        Ok(_) => test_print(&format!("Exported: {}/nsl_{:02}_global_info.json and .txt\n", config.output_dir, target_size)),
        Err(e) => test_print(&format!("Warning: Failed to export JSON/TXT: {}\n", e)),
    }
    
    // Save history at the end
    test_print(&format!("\nSaving historical state for size {}...", target_size));
    let history_config = ProcessingConfig {
        mode: ProcessingMode::SaveHistory { size: target_size },
        input_dir: config.output_dir.clone(),
        output_dir: String::new(),
        max_lists_per_file: config.max_lists_per_file,
        force_recount: false,
        keep_state: false,
    };
    match execute_mode(&history_config) {
        Ok(_) => test_print("Historical state saved successfully.\n"),
        Err(e) => test_print(&format!("Warning: Failed to save history: {}\n", e)),
    }
    
    Ok(format!("Unitary processing completed for size {} batch {}", unitary_size, unitary_batch))
}

/// Get directory path for a given size in cascade mode
/// Returns (input_dir, output_dir) for the given output size
fn get_cascade_directories(root_directory: &str, input_size: u8) -> (String, String) {
    use std::path::Path;
    
    let output_size = input_size + 1;
    
    // Input directory pattern
    let input_dir = if input_size == 12 {
        // Size 12 comes from 11_to_12
        Path::new(root_directory).join("11_to_12")
    } else if input_size == 13 {
        // Size 13 comes from 12_to_13c (12 doesn't have 'c')
        Path::new(root_directory).join("12_to_13c")
    } else {
        // Size 14+ comes from {size-1}c_to_{size}c
        Path::new(root_directory).join(format!("{}c_to_{}c", input_size - 1, input_size))
    };
    
    // Output directory pattern
    let output_dir = if output_size == 13 {
        // Size 13 goes to 12_to_13c
        Path::new(root_directory).join("12_to_13c")
    } else {
        // Size 14+ goes to {size-1}c_to_{size}c
        Path::new(root_directory).join(format!("{}c_to_{}c", output_size - 1, output_size))
    };
    
    (
        input_dir.to_string_lossy().to_string(),
        output_dir.to_string_lossy().to_string()
    )
}

/// Find the highest source batch number in the output directory
/// Returns None if no files found, or the max source batch number
fn find_max_source_batch(output_dir: &str, output_size: u8) -> Option<u32> {
    use std::fs;
    
    let entries = match fs::read_dir(output_dir) {
        Ok(e) => e,
        Err(_) => return None,
    };
    
    let pattern = format!("_to_{:02}_batch_", output_size);
    let mut max_source_batch: Option<u32> = None;
    
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with("nsl_") && name.contains(&pattern) && name.ends_with(".rkyv") {
                // Parse source batch from filename: nsl_{size}_batch_{source_batch}_to_...
                if let Some(to_pos) = name.find("_to_") {
                    let before_to = &name[..to_pos];
                    if let Some(batch_pos) = before_to.rfind("_batch_") {
                        let batch_str = &before_to[batch_pos + 7..];
                        if let Ok(source_batch) = batch_str.parse::<u32>() {
                            max_source_batch = Some(
                                max_source_batch.map_or(source_batch, |current| current.max(source_batch))
                            );
                        }
                    }
                }
            }
        }
    }
    
    max_source_batch
}

/// Execute save-history mode: merge current state with historical state
fn execute_save_history_mode(input_dir: &str, size: u8) -> Result<String, String> {
    use crate::file_info::GlobalFileState;
    use std::path::Path;
    
    test_print(&format!("\n================================================================="));
    test_print(&format!("SAVE HISTORY MODE - Size {}", size));
    test_print(&format!("Directory: {}", input_dir));
    test_print(&format!("=================================================================\n"));
    
    // Load current state
    test_print("Loading current state...");
    let current_state = GlobalFileState::from_sources(input_dir, size)
        .map_err(|e| format!("Failed to load current state: {}", e))?;
    let current_count = current_state.entries().len();
    test_print(&format!("   Current state: {} entries", current_count));
    
    // Try to load existing history
    let history_rkyv_path = Path::new(input_dir).join(format!("nsl_{:02}_global_info_history.rkyv", size));
    let history_json_path = Path::new(input_dir).join(format!("nsl_{:02}_global_info_history.json", size));
    
    let mut historical_state = if history_rkyv_path.exists() {
        test_print("Loading existing history from rkyv...");
        GlobalFileState::from_history_file(input_dir, size, "rkyv")
            .map_err(|e| format!("Failed to load history from rkyv: {}", e))?
    } else if history_json_path.exists() {
        test_print("Loading existing history from JSON...");
        GlobalFileState::from_history_file(input_dir, size, "json")
            .map_err(|e| format!("Failed to load history from JSON: {}", e))?
    } else {
        test_print("No existing history found, creating new historical state...");
        GlobalFileState::new(input_dir, size)
    };
    
    let initial_history_count = historical_state.entries().len();
    test_print(&format!("   Historical state: {} entries", initial_history_count));
    
    // Remove entries from history that were removed from current state
    let removed_entries = current_state.removed_entries();
    if !removed_entries.is_empty() {
        test_print(&format!("\nRemoving {} consumed files from history...", removed_entries.len()));
        let mut removed_count = 0;
        for (src, tgt, filename) in removed_entries.iter() {
            if historical_state.has_entry(filename, *src, *tgt) {
                historical_state.remove_file(filename, *src, *tgt);
                removed_count += 1;
            }
        }
        test_print(&format!("   Removed: {} entries from history", removed_count));
    }
    
    // Merge current state into historical state
    test_print("\nMerging current state into history...");
    let mut added_count = 0;
    let mut updated_count = 0;
    
    for ((src, tgt, filename), info) in current_state.entries().iter() {
        if historical_state.has_entry(filename, *src, *tgt) {
            // Entry exists, update it (in case counts changed)
            historical_state.update_entry(
                filename,
                *src,
                *tgt,
                info.nb_lists_in_file,
                info.compacted,
                info.file_size_bytes,
                info.modified_timestamp,
            );
            updated_count += 1;
        } else {
            // New entry, add it
            historical_state.register_file(
                filename,
                *src,
                *tgt,
                info.nb_lists_in_file,
                info.compacted,
                info.file_size_bytes,
                info.modified_timestamp,
            );
            added_count += 1;
        }
    }
    
    let final_history_count = historical_state.entries().len();
    let removed_count = removed_entries.len();
    
    test_print(&format!("   Added: {} new entries", added_count));
    test_print(&format!("   Updated: {} existing entries", updated_count));
    if removed_count > 0 {
        test_print(&format!("   Removed: {} consumed entries", removed_count));
    }
    test_print(&format!("   Total historical entries: {}", final_history_count));
    
    // Save historical state as triplet
    test_print("\nSaving historical state...");
    historical_state.flush_as_history()
        .map_err(|e| format!("Failed to save historical state: {}", e))?;
    historical_state.export_human_readable_as_history()
        .map_err(|e| format!("Failed to export historical JSON/TXT: {}", e))?;
    
    test_print(&format!("   Saved: {}", history_rkyv_path.display()));
    test_print(&format!("   Saved: {}", history_json_path.display()));
    test_print(&format!("   Saved: {}", Path::new(input_dir).join(format!("nsl_{:02}_global_info_history.txt", size)).display()));
    
    test_print(&format!("\n================================================================="));
    test_print(&format!("SAVE HISTORY COMPLETED"));
    test_print(&format!("=================================================================\n"));
    
    let removed_count = removed_entries.len();
    if removed_count > 0 {
        Ok(format!("History saved: {} total entries ({} added, {} updated, {} removed)", 
            final_history_count, added_count, updated_count, removed_count))
    } else {
        Ok(format!("History saved: {} total entries ({} added, {} updated)", 
            final_history_count, added_count, updated_count))
    }
}

/// Execute cascade mode: process all sizes starting from a given input size
fn execute_cascade_mode(starting_input_size: u8, root_directory: &str, max_lists_per_file: u64) -> Result<String, String> {
    use std::path::Path;
    
    test_print(&format!("\n================================================================="));
    test_print(&format!("CASCADE MODE - Starting from input size {}", starting_input_size));
    test_print(&format!("Root directory: {}", root_directory));
    test_print(&format!("=================================================================\n"));
    
    let mut total_sizes_processed = 0;
    let mut total_commands_executed = 0;
    
    // Process each size from starting_input_size to 19 (output sizes 13 to 20)
    for input_size in starting_input_size..=19 {
        let output_size = input_size + 1;
        
        test_print(&format!("\n--- Step {}: Processing size {} (from input size {}) ---",
            input_size - starting_input_size + 1, output_size, input_size));
        
        // Get directories
        let (input_dir, output_dir) = get_cascade_directories(root_directory, input_size);
        
        // Check if input directory exists
        if !Path::new(&input_dir).exists() {
            test_print(&format!("   Input directory does not exist: {}", input_dir));
            test_print(&format!("   Skipping size {}", output_size));
            continue;
        }
        
        // Check if output directory exists, create if not
        if !Path::new(&output_dir).exists() {
            test_print(&format!("   Output directory does not exist, creating: {}", output_dir));
            std::fs::create_dir_all(&output_dir)
                .map_err(|e| format!("Failed to create output directory {}: {}", output_dir, e))?;
        }
        
        // Find the last processed batch
        let last_processed = find_max_source_batch(&output_dir, output_size);
        let next_batch = match last_processed {
            Some(batch) => batch + 1,
            None => 0,
        };
        
        test_print(&format!("   Last processed input batch: {}",
            last_processed.map_or("none".to_string(), |b| format!("{:06}", b))));
        test_print(&format!("   Next batch to process: {:06}", next_batch));
        test_print(&format!("   Input directory:  {}", input_dir));
        test_print(&format!("   Output directory: {}", output_dir));
        
        test_print(&format!("\n   Processing: --size {} {} -i \"{}\" -o \"{}\"\n",
            output_size, next_batch, input_dir, output_dir));
        
        // Build configuration for this size (call internal functions directly)
        let size_config = ProcessingConfig {
            mode: ProcessingMode::Size { 
                size: output_size, 
                start_batch: if next_batch > 0 { Some(next_batch) } else { None }
            },
            input_dir: input_dir.clone(),
            output_dir: output_dir.clone(),
            max_lists_per_file,
            force_recount: false,
            keep_state: false,
        };
        
        // Execute the size mode directly (same as if user entered the command)
        match execute_mode(&size_config) {
            Ok(_) => {
                test_print(&format!("\n   ✓ Size {} processing completed successfully\n", output_size));
                
                // Save history for this size
                test_print(&format!("   Saving historical state for size {}...", output_size));
                let history_config = ProcessingConfig {
                    mode: ProcessingMode::SaveHistory { size: output_size },
                    input_dir: output_dir.clone(),
                    output_dir: String::new(),
                    max_lists_per_file,
                    force_recount: false,
                    keep_state: false,
                };
                match execute_mode(&history_config) {
                    Ok(_) => test_print("   Historical state saved.\n"),
                    Err(e) => test_print(&format!("   Warning: Failed to save history: {}\n", e)),
                }
                
                total_sizes_processed += 1;
            }
            Err(e) => {
                test_print(&format!("\n   ✗ Size {} processing failed: {}\n", output_size, e));
                test_print(&format!("   Stopping cascade at this point.\n"));
                break;
            }
        }
        
        total_commands_executed += 1;
    }
    
    test_print(&format!("\n================================================================="));
    test_print(&format!("CASCADE MODE COMPLETED"));
    test_print(&format!("Sizes processed: {}", total_sizes_processed));
    test_print(&format!("Commands executed: {}", total_commands_executed));
    test_print(&format!("=================================================================\n"));
    
    Ok(format!("Cascade mode completed: {} sizes processed", total_sizes_processed))
}

/// Execute default mode: process the whole pipeline (seeds + sizes 4 to 20)
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
    test_print("   - will create 193.375.848.191 no-set-lists with 10 cards");
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
    for size in 3..19 {
        let target_size = size + 1;
        let mut global_state = GlobalFileState::from_sources(&config.output_dir, target_size)
            .map_err(|e| format!("Failed to load global state: {}", e))?;
        test_print(&format!("\nStart processing files to create no-set-lists of size {}:", target_size));
        no_set_lists.process_all_files_of_current_size_n(size, &config.max_lists_per_file, Some(&mut global_state));
        
        // Export human-readable state files for this size
        test_print(&format!("Exporting global state files for size {}...", target_size));
        match global_state.export_human_readable() {
            Ok(_) => test_print(&format!("Exported: {}/nsl_{:02}_global_info.json and .txt", config.output_dir, target_size)),
            Err(e) => test_print(&format!("Warning: Failed to export JSON/TXT: {}", e)),
        }
    }
    
    Ok("Default pipeline completed (sizes 3-20)".to_string())
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

    banner(concat!("Funny Set Exploration [0.4.14]"));
    
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
