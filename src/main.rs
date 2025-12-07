/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets
///
/// UNIFIED VERSION - Supports both v0.2.2 and v0.3.0 via CLI flag
/// 
/// CLI Usage:
///   funny.exe -v 2 --size 5 -o T:\data\funny_set_exploration  # v0.2.2 (heap-based)
///   funny.exe -v 3 --size 5 -o T:\data\funny_set_exploration  # v0.3.0 (stack-optimized)
///   funny.exe -v 2                                             # v0.2.2 default mode
///   funny.exe -v 3                                             # v0.3.0 default mode
///
/// Arguments:
///   -v, --version <2|3>  Implementation version (required)
///                        2: v0.2.2 - Heap-based with Vec (backward compatible)
///                        3: v0.3.0 - Stack-optimized with arrays (3-8x faster)
///   --size, -s <SIZE>    Target size to build (4-12, optional)
///                        If omitted, runs default behavior
///   --output-path, -o    Optional: Directory for output files
///                        Defaults to current directory
///
/// Version Differences:
///   v0.2.2: Uses NList with Vec, creates .rkyv files, backward compatible
///   v0.3.0: Uses NoSetList with arrays, creates .nsl files, NOT compatible

mod utils;
mod set;
mod nlist;
mod no_set_list;
mod list_of_nlists;
mod list_of_nsl;

use clap::Parser;
use crate::utils::*;

/// CLI arguments structure
#[derive(Parser, Debug)]
#[command(name = "funny_set_exploration")]
#[command(about = "Generate no-set lists for the Set card game", long_about = None)]
struct Args {
    /// Implementation version: 2 (heap-based) or 3 (stack-optimized)
    #[arg(short = 'v', long, value_parser = clap::value_parser!(u8).range(2..=3))]
    version: u8,

    /// Target size for the no-set lists (4-12)
    /// 
    /// If not provided, runs the default behavior (creates seeds + sizes 4-6)
    /// - Size 4: Builds from seed lists (size 3)
    /// - Size 5+: Requires files from previous size
    #[arg(short, long, value_parser = clap::value_parser!(u8).range(4..=12))]
    size: Option<u8>,

    /// Output directory path (optional)
    /// 
    /// Examples:
    ///   Windows: T:\data\funny_set_exploration
    ///   Linux:   /mnt/nas/data/funny_set_exploration
    ///   Relative: ./output
    #[arg(short, long)]
    output_path: Option<String>,
}

fn main() {
    // Parse command-line arguments
    let args = Args::parse();

    /// Max number of n-list saved per file
    ///     - I usually set it at 20 millions.
    ///     - With rkyv: each file will be about 1.9GB (down from 3.2GB with 
    ///       bincode)
    ///     - Peak RAM usage: ~10.5GB (down from ~13.5GB with bincode)
    ///     - Files are saved as .rkyv format (memory-mapped, zero-copy)
    ///     - Old .bin files (bincode) are still readable for backward 
    ///       compatibility
    const MAX_NLISTS_PER_FILE: u64 = 20_000_000;

    debug_print_on();
    debug_print_off();
    test_print_off();
    test_print_on();

    // Dispatch to appropriate version
    match args.version {
        2 => run_version_2(args.size, args.output_path, MAX_NLISTS_PER_FILE),
        3 => run_version_3(args.size, args.output_path, MAX_NLISTS_PER_FILE),
        _ => unreachable!("clap should prevent other values"),
    }
}

/// Run version 0.2.2 (heap-based with Vec)
fn run_version_2(size: Option<u8>, output_path: Option<String>, max_per_file: u64) {
    use crate::list_of_nlists::{ListOfNlist, created_a_total_of};

    banner("Funny Set Exploration - v0.2.2 (Heap-Based)");
    
    if let Some(target_size) = size {
        // =====================================================================
        // CLI MODE: Process specific size
        // =====================================================================
        test_print(&format!("v0.2.2 CLI Mode: Target size = {} cards", target_size));
        
        if let Some(ref path) = output_path {
            test_print(&format!("Output directory: {}", path));
        } else {
            test_print("Output directory: current directory");
        }
        test_print("\n======================\n");

        // Initialize ListOfNlist with optional custom path
        let mut no_set_lists: ListOfNlist = match output_path {
            Some(path) => ListOfNlist::with_path(&path),
            None => ListOfNlist::new(),
        };

        // Handle size 4: need to create seed lists first
        if target_size == 4 {
            test_print("Creating seed lists (size 3)...");
            no_set_lists.create_seed_lists();
            test_print("Seed lists created successfully.\n");
        }

        // Process from (size - 1) to size
        let source_size = target_size - 1;
        test_print(&format!("Processing files nlist_{:02}_batch_*.rkyv to create no-set-lists of size {}:", 
            source_size, target_size));
        
        let nb_new = no_set_lists.process_all_files_of_current_size_n(
            source_size, 
            &max_per_file
        );
        
        created_a_total_of(nb_new, target_size);
        test_print(&format!("\nCompleted! Generated files: nlist_{:02}_batch_*.rkyv", target_size));
    } else {
        // =====================================================================
        // DEFAULT MODE: Original behavior
        // =====================================================================
        test_print("   - will create         58.896 no-set-lists with  3 cards");
        test_print("   - will create      1.004.589 no-set-lists with  4 cards");
        test_print("   - will create     13.394.538 no-set-lists with  5 cards");
        test_print("   - will create    141.370.218 no-set-lists with  6 cards");
        test_print("   - will create  1.180.345.041 no-set-lists with  7 cards");
        test_print("   - will create  7.920.450.378 no-set-lists with  8 cards");
        test_print("\n======================\n");

        // Initialize with output path if provided
        let mut no_set_lists: ListOfNlist = match output_path {
            Some(path) => ListOfNlist::with_path(&path),
            None => ListOfNlist::with_path(r"T:\data\funny_set_exploration"),
        };

        // create all seed lists (no-set-lists of size 3)
        no_set_lists.create_seed_lists();

        // expand from seed_lists to Nlist of size 4, 5, 6...
        for size in 3..6 {
        //for size in 6..9 {
        //for size in 9..12 {
            test_print(&format!("Start processing the files to create no-set-lists \
                of size {}:", size+1));
            let nb_new = no_set_lists.process_all_files_of_current_size_n(size, 
                &max_per_file);
            created_a_total_of(nb_new, size+1);
        }
    }
}

/// Run version 0.3.0 (stack-optimized with arrays)
fn run_version_3(size: Option<u8>, output_path: Option<String>, max_per_file: u64) {
    use crate::list_of_nsl::{ListOfNSL, created_a_total_of};

    banner("Funny Set Exploration - v0.3.0 (Stack-Optimized)");
    
    if let Some(target_size) = size {
        // =====================================================================
        // CLI MODE: Process specific size
        // =====================================================================
        test_print(&format!("v0.3.0 CLI Mode: Target size = {} cards", target_size));
        test_print("Using STACK-OPTIMIZED algorithm (zero heap allocations, 3-8x faster)");
        
        if let Some(ref path) = output_path {
            test_print(&format!("Output directory: {}", path));
        } else {
            test_print("Output directory: current directory");
        }
        test_print("\n======================\n");

        // Initialize ListOfNSL with optional custom path
        let mut no_set_lists: ListOfNSL = match output_path {
            Some(path) => ListOfNSL::with_path(&path),
            None => ListOfNSL::new(),
        };

        // Handle size 4: need to create seed lists first
        if target_size == 4 {
            test_print("Creating seed lists (size 3) using STACK ALLOCATION...");
            no_set_lists.create_seed_lists();
            test_print("Seed lists created successfully.\n");
        }

        // Process from (size - 1) to size
        let source_size = target_size - 1;
        test_print(&format!("Processing files nlist_{:02}_batch_*.nsl to create no-set-lists of size {}:", 
            source_size, target_size));
        
        let nb_new = no_set_lists.process_all_files_of_current_size_n(
            source_size, 
            &max_per_file
        );
        
        created_a_total_of(nb_new, target_size);
        test_print(&format!("\nCompleted! Generated files: nlist_{:02}_batch_*.nsl", target_size));
    } else {
        // =====================================================================
        // DEFAULT MODE: Stack-optimized behavior
        // =====================================================================
        test_print("   - will create         58.896 no-set-lists with  3 cards (STACK)");
        test_print("   - will create      1.004.589 no-set-lists with  4 cards (STACK)");
        test_print("   - will create     13.394.538 no-set-lists with  5 cards (STACK)");
        test_print("   - will create    141.370.218 no-set-lists with  6 cards (STACK)");
        test_print("   - will create  1.180.345.041 no-set-lists with  7 cards (STACK)");
        test_print("   - will create  7.920.450.378 no-set-lists with  8 cards (STACK)");
        test_print("\n   Performance: 3-8x faster than v0.2.2 (zero heap allocations)\n");
        test_print("\n======================\n");

        // Initialize with output path if provided
        let mut no_set_lists: ListOfNSL = match output_path {
            Some(path) => ListOfNSL::with_path(&path),
            None => ListOfNSL::with_path(r"T:\data\funny_set_exploration"),
        };

        // Create all seed lists using STACK ALLOCATION
        test_print("Creating seed lists with STACK optimization...");
        no_set_lists.create_seed_lists();

        // Expand from seed_lists to NoSetList of size 4, 5, 6...
        for size in 3..6 {
            test_print(&format!("\nStart processing files to create no-set-lists of size {}:", size+1));
            test_print("Using STACK-OPTIMIZED algorithm (zero heap allocations in core loop)");
            let nb_new = no_set_lists.process_all_files_of_current_size_n(size, 
                &max_per_file);
            created_a_total_of(nb_new, size+1);
        }
    }
}