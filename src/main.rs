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
    const MAX_NLISTS_PER_FILE: u64 = 20_000_000;


    debug_print_off();
    test_print_on();
    test_print("Funny Set Exploration");
    test_print("======================");
    test_print("   - will create       58.896 no-set-lists with  3 cards");
    test_print("   - will create    1.098.240 no-set-lists with  4 cards");
    test_print("   - will create   13.394.538 no-set-lists with  5 cards");
    test_print("   - will create  155.769.345 no-set-lists with  6 cards");
    test_print("======================\n");

    // Create the ListOfNlists used for all iterations
    let mut no_set_lists: ListOfNlist = ListOfNlist::new();

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