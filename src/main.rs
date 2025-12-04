/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets

mod is_set;
mod n_list;

use crate::n_list::*;

fn main() {
    println!("Funny Set Exploration");
    println!("======================");
    println!("   - will create       58.896 no-set-lists with  3 cards");
    println!("   - will create    1.098.240 no-set-lists with  4 cards");
    println!("   - will create   13.394.538 no-set-lists with  5 cards");
    println!("======================");
    

    // Create all seed lists (no-set-03 lists)
    let no_set_03_lists: Vec<NList> = n_list::create_all_03_no_set_lists();
    
    if !n_list::save_to_file(
        &no_set_03_lists, 
        &filename(3, 0)) {
        eprintln!("Failed to save the seed lists");
        return;
    }
    println!("Created {} no-set-03 lists", no_set_03_lists.len());


    // expand to 4 cards no-set lists
    let mut no_set_04_lists: ListOfNlist = ListOfNlist::new(3);
    no_set_04_lists.process_all_files_for_size_n(3);

    // expand to 5 cards no-set lists
    let mut no_set_05_lists: ListOfNlist = ListOfNlist::new(4);
    no_set_05_lists.process_all_files_for_size_n(4);

    // expand to 6 cards no-set lists
    let mut no_set_06_lists: ListOfNlist = ListOfNlist::new(5);
    no_set_06_lists.process_all_files_for_size_n(5);


}