/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets


fn main() {
    println!("Funny Set Exploration");

    let mut no_set_03_lists = n_list::create_all_03_no_set_lists();
    println!("Created {} no-set-03 lists", no_set_03_lists.len());
    for i in 0-12 {
        let nlist = &no_set_03_lists[i];
        println!("{}", nlist.to_string());
    }
}