/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets
///
/// VERSION 0.2.1 - Now using rkyv for zero-copy serialization
/// 
/// Key improvements:
/// - Memory-mapped file access (zero-copy reading)
/// - 10-100x faster file reads
/// - ~50% reduction in peak memory usage
/// - Backward compatible with old .bin files
///
/// CLI Usage:
///   cargo run                          # Default: create seeds + sizes 4-6
///   cargo run -- --size 5              # Build size 5 from existing size 4 files
///   cargo run -- --size 4              # Build size 4 from seed lists
///   cargo run -- --size 7 -o T:\output # With custom output directory
///
/// Arguments:
///   --size, -s <SIZE>    Target size to build (4-12, optional)
///                        If omitted, runs default behavior
///   --output-path, -o    Optional: Directory for output files
///                        Defaults to current directory

mod utils;
mod set;
mod nlist;
mod list_of_nlists;

use clap::Parser;
use crate::utils::*;
use crate::list_of_nlists::{ListOfNlist, created_a_total_of};

/// CLI arguments structure
#[derive(Parser, Debug)]
#[command(name = "funny_set_exploration")]
#[command(about = "Generate no-set lists for the Set card game", long_about = None)]
struct Args {
    /// Target size for the no-set lists (4-12)
    /// 
    /// If not provided, runs the default behavior (creates seeds + sizes 4-6)
    /// - Size 4: Builds from seed lists (size 3)
    /// - Size 5+: Requires nlist_(size-1)_batch_*.rkyv files
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

    debug_print_off();
    test_print_on();
    banner("Funny Set Exploration");
    
    // Check if CLI mode (--size argument provided) or default mode
    if let Some(target_size) = args.size {
        // =====================================================================
        // CLI MODE: Process specific size
        // =====================================================================
        test_print(&format!("CLI Mode: Target size = {} cards", target_size));
        
        if let Some(ref path) = args.output_path {
            test_print(&format!("Output directory: {}", path));
        } else {
            test_print("Output directory: current directory");
        }
        test_print("\n======================\n");

        // Initialize ListOfNlist with optional custom path
        let mut no_set_lists: ListOfNlist = match args.output_path {
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
            &MAX_NLISTS_PER_FILE
        );
        
        created_a_total_of(nb_new, target_size);
        test_print(&format!("\nCompleted! Generated files: nlist_{:02}_batch_*.rkyv", target_size));
    } else {
        // =====================================================================
        // DEFAULT MODE: Original behavior
        // =====================================================================
        test_print("   - will create       58.896 no-set-lists with  3 cards");
        test_print("   - will create    1.004.589 no-set-lists with  4 cards");
        test_print("   - will create   14.399.127 no-set-lists with  5 cards");
        test_print("   - will create  155.769.345 no-set-lists with  6 cards");
        test_print("   - will create  ___.___.___ no-set-lists with  7 cards");
        test_print("   - will create  ___.___.___ no-set-lists with  8 cards");
        test_print("   - will create  ___.___.___ no-set-lists with  9 cards");
        test_print("   - will create  ___.___.___ no-set-lists with  10 cards");
        test_print("   - will create  ___.___.___ no-set-lists with  11 cards");
        test_print("   - will create  ___.___.___ no-set-lists with  12 cards");
        test_print("\n======================\n");

        // ========================================================================
        // CONFIGURE OUTPUT DIRECTORY
        // ========================================================================
        // Option 1: Use current directory (default)
        // let mut no_set_lists: ListOfNlist = ListOfNlist::new();
        
        // Option 2: Use a custom path on Windows (uncomment to use)
        // Example: NAS drive mapped to T:\data\funny_set_exploration
        let mut no_set_lists: ListOfNlist = ListOfNlist::with_path(
            r"T:\data\funny_set_exploration");
        
        // Option 3: Use a custom path on Linux (uncomment to use)
        // Example: NAS mounted at /mnt/nas/data/funny_set_exploration
        // let mut no_set_lists: ListOfNlist = ListOfNlist::with_path("/mnt/nas/data/funny_set_exploration");
        
        // Option 4: Use a relative subdirectory
        // let mut no_set_lists: ListOfNlist = ListOfNlist::with_path("output");
        
        // Note: Make sure the directory exists before running!
        // ========================================================================

        // create all seed lists (no-set-lists of size 3)
        no_set_lists.create_seed_lists();

        // expand from seed_lists to Nlist of size 4, 5, 6...
        for size in 3..6 {
        //for size in 6..9 {
        //for size in 9..12 {
            test_print(&format!("Start processing the files to create no-set-lists \
                of size {}:", size+1));
            let nb_new = no_set_lists.process_all_files_of_current_size_n(size, 
                &MAX_NLISTS_PER_FILE);
            created_a_total_of(nb_new, size+1);
        }


        // expand to 4 cards no-set lists
        //let mut no_set_04_lists: ListOfNlist = ListOfNlist::new(3);
        //no_set_04_lists.process_all_files_for_size_n(3);
    }
}