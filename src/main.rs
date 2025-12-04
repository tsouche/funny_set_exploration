/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets

mod is_set;
mod n_list;

use crate::n_list::*;

fn main() {
    println!("Funny Set Exploration");
    println!("======================");

    // Create all seed lists (no-set-03 lists)
    let no_set_03_lists: Vec<NList> = n_list::create_all_03_no_set_lists();
    println!("Created {} no-set-03 lists", no_set_03_lists.len());

    // expand to 4 cards no-set lists
    let mut no_set_04_lists: Vec<NList> = Vec::new();
    for list in &no_set_03_lists {
        no_set_04_lists.extend(list.build_n_plus_1_no_set_lists());
    }
    println!("From first no-set-03 list, created {} no-set-04 lists", no_set_04_lists.len());
}