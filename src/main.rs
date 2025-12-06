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
///   cargo run -- <size> [output_path]
///
/// Arguments:
///   <size>         Target size to build (4-12)
///                  Size 4 requires seed lists (size 3)
///                  Size n>4 requires files nlist_(n-1)_batch_*.rkyv
///   [output_path]  Optional: Directory for output files
///                  Defaults to current directory

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
    /// - Size 4: Builds from seed lists (size 3)
    /// - Size 5+: Requires nlist_(size-1)_batch_*.rkyv files
    #[arg(value_parser = clap::value_parser!(u8).range(4..=12))]
    size: u8,

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