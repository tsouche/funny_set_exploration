/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets

mod utils;
mod set;
mod nlist;
mod list_of_nlists;

use crate::utils::*;
use crate::list_of_nlists::{ListOfNlist, created_a_total_of};

fn main() {

    /// Max number of n-list saved per file
    ///     - I usually set it at 20 millions.
    ///     - With a limit of 20 million n-lists per file, each file will be 
    ///       about 4GB and the RAM usage grow up to ~13.5GB when a batch is 
    ///       about to be saved to disk
    ///     - Once the batch is saved to disk, the RAM usage goes down to less
    ///       than 5 GB.
    const MAX_NLISTS_PER_FILE: u64 = 20_000_000;


    debug_print_off();
    test_print_on();
    banner("Funny Set Exploration");
    test_print("   - will create       58.896 no-set-lists with  3 cards");
    test_print("   - will create    1.098.240 no-set-lists with  4 cards");
    test_print("   - will create   13.394.538 no-set-lists with  5 cards");
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
    //no_set_lists.create_seed_lists();

    // expand from seed_lists to Nlist of size 4, 5, 6...
    //for size in 3..6 {
    for size in 6..9 {
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