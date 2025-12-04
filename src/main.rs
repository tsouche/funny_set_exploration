/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets

mod is_set;
mod n_list;

use crate::n_list::*;

fn main() {
    println!("Funny Set Exploration");

    let no_set_03_lists = n_list::create_all_03_no_set_lists();
    println!("Created {} no-set-03 lists", no_set_03_lists.len());

    let print_count = std::cmp::min(no_set_03_lists.len(), 12);
    for i in 0..print_count {
        let nlist = &no_set_03_lists[i];
        println!("{}", nlist.to_string());
    }
    let no_set_03_list = no_set_03_lists[0].clone();
    let no_set_04_lists = no_set_03_list.build_n_plus_1_no_set_lists();
    println!("From first no-set-03 list, created {} no-set-04 lists", no_set_04_lists.len());
}