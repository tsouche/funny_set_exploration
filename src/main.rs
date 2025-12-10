/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets
///
/// Version 0.4.1 - Hybrid: Stack computation + Heap I/O + Multiple modes
/// 
/// CLI Usage:
///   funny.exe --size 5 -o T:\data\funny_set_exploration      # Build size 5 from size 4
///   funny.exe --size 5-7 -o T:\data\funny_set_exploration       # Build sizes 5, 6, and 7
///   funny.exe --restart 5 2 -o T:\data\funny_set_exploration    # Restart from size 5 batch 2
///   funny.exe --unitary 5 2 -o T:\data\funny_set_exploration    # Process only size 5 batch 2
///   funny.exe --count 6 -o T:\data\funny_set_exploration        # Count size 6 files
///   funny.exe                                                # Default mode (sizes 4-18)
///
/// Arguments:
///   --size, -s <SIZE>        Target size to build (4-18, or range like 5-7)
///                            If omitted, runs default behavior (creates seeds + sizes 4-18)
///   --restart <SIZE> <BATCH>   Restart from specific input batch through size 18
///   --unitary <SIZE> <BATCH>   Process only one specific input batch (unitary processing)
///   --count <SIZE>             Count existing files and create summary report
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
    /// If not provided, runs the default behavior (creates seeds + sizes 4-18)
    /// - Single size: "5" builds size 5 from size 4 files
    /// - Range: "5-7" builds sizes 5, 6, and 7 sequentially
    /// - Size 4: Builds from seed lists (size 3)
    /// - Size 5+: Requires files from previous size
    #[arg(short, long, conflicts_with_all = ["restart", "unitary"])]
    size: Option<String>,

    /// Restart from specific input file: <SIZE> <BATCH>
    /// 
    /// SIZE refers to the INPUT size. Processes from this batch onwards.
    /// 
    /// Examples:
    ///   --restart 5 2   Load input size 5 batch 2, continue through size 18
    ///   --restart 7 0   Load input size 7 batch 0, continue through size 18
    /// 
    /// By default, reads baseline counts from count file (no_set_list_count_XX.txt).
    /// Use --force to regenerate count file by scanning all files.
    #[arg(long, num_args = 2, value_names = ["SIZE", "BATCH"], conflicts_with_all = ["size", "count", "unitary"])]
    restart: Option<Vec<u32>>,

    /// Process a single input batch (unitary processing): <SIZE> <BATCH>
    /// 
    /// SIZE refers to the INPUT size. Processes ONLY this specific batch.
    /// This is the ONLY canonical way to overwrite/fix a defective output file.
    /// Output files from this batch will be regenerated.
    /// 
    /// Examples:
    ///   --unitary 5 2    Reprocess input size 5 batch 2 only (creates size 6 outputs)
    ///   --unitary 7 0    Reprocess input size 7 batch 0 only (creates size 8 outputs)
    /// 
    /// Use --force to regenerate count file first (recalculates baseline).
    #[arg(long, num_args = 2, value_names = ["SIZE", "BATCH"], conflicts_with_all = ["size", "count", "restart"])]
    unitary: Option<Vec<u32>>,

    /// Force regeneration of count file when using --restart or --unitary
    /// 
    /// By default, restart/unitary modes read the existing count file for baseline counts.
    /// This flag forces a full file scan to regenerate the count file first.
    #[arg(long)]
    force: bool,

    /// Count existing files for a specific size and create summary report
    /// 
    /// Examples:
    ///   --count 6   Count all size 6 files and create no_set_list_count_06.txt
    /// 
    /// This scans all files, counts lists, and creates a summary report
    /// without processing any new lists
    #[arg(long, conflicts_with_all = ["size", "restart", "unitary", "compact"])]
    count: Option<u8>,

    /// Compact small output files into larger batches: <SIZE>
    /// 
    /// SIZE refers to the OUTPUT size to compact. Reads all files for this size,
    /// consolidates them into 10M-entry batches, and replaces original files.
    /// 
    /// New filename format: nsl_compacted_{size:02}_batch_{batch:05}_from_{first_source_batch:05}.rkyv
    /// 
    /// Examples:
    ///   --compact 8   Compact all size 8 files into 10M-entry batches
    ///   --compact 12  Compact all size 12 files into 10M-entry batches
    /// 
    /// Use when later processing waves create many small files (ratio < 1.0).
    /// Original files are deleted after successful compaction.
    #[arg(long, conflicts_with_all = ["size", "restart", "unitary", "count"])]
    compact: Option<u8>,

    /// Output directory path (optional)
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

fn main() {
    // Parse command-line arguments
    let args = Args::parse();

    /// Max number of n-list saved per file for v0.4.0
    /// - Each NoSetList: 792 bytes during compute (stack)
    /// - Each NoSetListSerialized: ~100 bytes after conversion (heap)
    /// - 20M entries × 100 bytes = ~2GB per file after serialization
    /// - Peak RAM during save: ~10.5GB (vec + archive + overhead)
    const MAX_NLISTS_PER_FILE: u64 = 10_000_000;

    debug_print_on();
    debug_print_off();
    test_print_off();
    test_print_on();

    // Parse size range if provided
    let size_range = if let Some(ref size_str) = args.size {
        match parse_size_range(size_str) {
            Ok(range) => Some(range),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    // Parse restart parameters if provided
    let restart_params = if let Some(ref restart_vec) = args.restart {
        if restart_vec.len() != 2 {
            eprintln!("Error: --restart requires exactly 2 arguments: SIZE BATCH");
            std::process::exit(1);
        }
        let size = restart_vec[0] as u8;
        let batch = restart_vec[1];  // u32 for 5-digit batch numbers
        if size < 4 || size > 18 {
            eprintln!("Error: Restart size {} out of range (4-18)", size);
            std::process::exit(1);
        }
        Some((size, batch))
    } else {
        None
    };

    // Parse unitary parameters if provided
    let unitary_params = if let Some(ref unitary_vec) = args.unitary {
        if unitary_vec.len() != 2 {
            eprintln!("Error: --unitary requires exactly 2 arguments: SIZE BATCH");
            std::process::exit(1);
        }
        let size = unitary_vec[0] as u8;
        let batch = unitary_vec[1];  // u32 for 5-digit batch numbers
        if size < 3 || size > 17 {
            eprintln!("Error: Unitary size {} out of range (3-17)", size);
            std::process::exit(1);
        }
        Some((size, batch))
    } else {
        None
    };

    use crate::list_of_nsl::{ListOfNSL, count_size_files, compact_size_files};

    banner("Funny Set Exploration)");
    
    // =====================================================================
    // COMPACT MODE: Consolidate small files into larger batches
    // =====================================================================
    if let Some(compact_size) = args.compact {
        // Initialize log file for compact mode
        init_log_file();
        
        if compact_size < 3 || compact_size > 18 {
            eprintln!("Error: Compact size {} out of range (3-18)", compact_size);
            std::process::exit(1);
        }
        
        test_print(&format!("COMPACT MODE: Consolidating files for size {}", compact_size));
        test_print("This will replace multiple small files with larger 10M-entry batches\n");
        
        let base_path = args.output_path.as_deref().unwrap_or(".");
        test_print(&format!("Directory: {}\n", base_path));
        
        match compact_size_files(base_path, compact_size, MAX_NLISTS_PER_FILE) {
            Ok(()) => {
                test_print("\nCompaction completed successfully!");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Error during compaction: {}", e);
                std::process::exit(1);
            }
        }
    }
    
    // =====================================================================
    // COUNT MODE: Count existing files for a specific size
    // =====================================================================
    if let Some(count_size) = args.count {
        // Initialize log file for count mode only
        init_log_file();
        
        if count_size < 3 || count_size > 18 {
            eprintln!("Error: Count size {} out of range (3-18)", count_size);
            std::process::exit(1);
        }
        
        test_print(&format!("COUNT MODE: Counting files for size {}", count_size));
        
        let base_path = args.output_path.as_deref().unwrap_or(".");
        test_print(&format!("Directory: {}\n", base_path));
        
        match count_size_files(base_path, count_size) {
            Ok(()) => {
                test_print("\nCount completed successfully!");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Error during count: {}", e);
                std::process::exit(1);
            }
        }
    }
    
    if let Some((restart_size, restart_batch)) = restart_params {
        // =====================================================================
        // RESTART MODE: Resume from specific batch
        // =====================================================================
        test_print(&format!("RESTART MODE: Resuming from size {} batch {}", restart_size, restart_batch));
        test_print(&format!("Will process through size 18"));
        test_print("Strategy: Stack computation + Heap I/O");
        test_print(&format!("Batch size: {} entries/file (~2GB, compact)", MAX_NLISTS_PER_FILE.separated_string()));
        
        let base_path = if let Some(ref path) = args.output_path {
            test_print(&format!("Output directory: {}", path));
            path.as_str()
        } else {
            test_print("Output directory: current directory");
            "."
        };
        
        // If force flag is set, regenerate count file for target size
        if args.force {
            test_print(&format!("\nFORCE MODE: Regenerating count file for size {}...", restart_size + 1));
            match count_size_files(base_path, restart_size + 1) {
                Ok(()) => test_print("Count file regenerated successfully\n"),
                Err(e) => {
                    eprintln!("Error regenerating count file: {}", e);
                    std::process::exit(1);
                }
            }
        }
        
        test_print("\n======================\n");

        // Initialize ListOfNSL with optional custom path
        let mut no_set_lists: ListOfNSL = match args.output_path {
            Some(path) => ListOfNSL::with_path(&path),
            None => ListOfNSL::new(),
        };

        // Process from restart point through size 18
        // restart_size is the INPUT size, so we create output starting from restart_size+1
        for target_size in (restart_size + 1)..=18 {
            let source_size = target_size - 1;
            
            if source_size == restart_size {
                // First iteration: start from specified batch of the input size
                test_print(&format!("Start processing files to create no-set-lists of size {} (from input batch {}):", 
                    target_size, restart_batch));
                let _nb_new = no_set_lists.process_from_batch(
                    source_size,  // Input size
                    restart_batch,
                    &MAX_NLISTS_PER_FILE
                );
            } else {
                // Subsequent iterations: process all files
                test_print(&format!("Start processing files to create no-set-lists of size {}:", target_size));
                let _nb_new = no_set_lists.process_all_files_of_current_size_n(
                    source_size, 
                    &MAX_NLISTS_PER_FILE
                );
            }
            
            test_print(&format!("\nCompleted size {}!\n", target_size));
        }
    } else if let Some((unitary_size, unitary_batch)) = unitary_params {
        // =====================================================================
        // UNITARY MODE: Process a single input batch
        // =====================================================================
        test_print(&format!("UNITARY MODE: Processing input size {} batch {}", unitary_size, unitary_batch));
        test_print(&format!("Output: size {} files", unitary_size + 1));
        test_print("Strategy: Stack computation + Heap I/O");
        test_print(&format!("Batch size: {} entries/file (~2GB, compact)", MAX_NLISTS_PER_FILE.separated_string()));
        
        let base_path = if let Some(ref path) = args.output_path {
            test_print(&format!("Output directory: {}", path));
            path.as_str()
        } else {
            test_print("Output directory: current directory");
            "."
        };
        
        // If force flag is set, regenerate count file for target size
        if args.force {
            test_print(&format!("\nFORCE MODE: Regenerating count file for size {}...", unitary_size + 1));
            match count_size_files(base_path, unitary_size + 1) {
                Ok(()) => test_print("Count file regenerated successfully\n"),
                Err(e) => {
                    eprintln!("Error regenerating count file: {}", e);
                    std::process::exit(1);
                }
            }
        }
        
        test_print("\n======================\n");

        // Initialize ListOfNSL with optional custom path
        let mut no_set_lists: ListOfNSL = match args.output_path {
            Some(path) => ListOfNSL::with_path(&path),
            None => ListOfNSL::new(),
        };

        // Process only the specified batch
        test_print(&format!("Processing input size {} batch {}:", unitary_size, unitary_batch));
        let _nb_new = no_set_lists.process_single_batch(
            unitary_size,
            unitary_batch,
            &MAX_NLISTS_PER_FILE
        );
        
        test_print(&format!("\nUnitary processing completed for size {} batch {}!\n", unitary_size, unitary_batch));
    } else if let Some((start_size, end_size)) = size_range {
        // =====================================================================
        // CLI MODE: Process size range
        // =====================================================================
        if start_size == end_size {
            test_print(&format!("Target size = {} cards", start_size));
        } else {
            test_print(&format!("Size range = {} to {} cards", start_size, end_size));
        }
        test_print("Strategy: Stack computation + Heap I/O");
        test_print(&format!("Batch size: {} entries/file (~2GB, compact)", MAX_NLISTS_PER_FILE.separated_string()));
        
        if let Some(ref path) = args.output_path {
            test_print(&format!("Output directory: {}", path));
        } else {
            test_print("Output directory: current directory");
        }
        test_print("\n======================\n");

        // Initialize ListOfNSL with optional custom path
        let mut no_set_lists: ListOfNSL = match args.output_path {
            Some(path) => ListOfNSL::with_path(&path),
            None => ListOfNSL::new(),
        };

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
            
            let _nb_new = no_set_lists.process_all_files_of_current_size_n(
                source_size, 
                &MAX_NLISTS_PER_FILE
            );
            
            test_print(&format!("\nCompleted size {}! Generated files: no-set-list_{:02}_batch_*.rkyv\n", 
                target_size, target_size));
        }
    } else {
        // =====================================================================
        // DEFAULT MODE - process the whole pipeline: seeds + sizes 4 to 18
        // =====================================================================
        test_print("   - will create          58.896 no-set-lists with  3 cards");
        test_print("   - will create       1.004.589 no-set-lists with  4 cards");
        test_print("   - will create      13.394.538 no-set-lists with  5 cards");
        test_print("   - will create     141.370.218 no-set-lists with  6 cards");
        test_print("   - will create   1.180.345.041 no-set-lists with  7 cards");
        test_print("   - will create   7.920.450.378 no-set-lists with  8 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with  9 cards");
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

        // Initialize with output path if provided
        let mut no_set_lists: ListOfNSL = match args.output_path {
            Some(path) => ListOfNSL::with_path(&path),
            None => ListOfNSL::with_path(r"T:\data\funny_set_exploration"),
        };

        // Create all seed lists
        test_print("Creating seed lists...");
        no_set_lists.create_seed_lists();

        // Expand from seed_lists to size 4, 5, 6...
        for size in 3..17 {
            test_print(&format!("\nStart processing files to create no-set-lists of size {}:", size+1));
            let _nb_new = no_set_lists.process_all_files_of_current_size_n(size, 
                &MAX_NLISTS_PER_FILE);
        }
    }
}
