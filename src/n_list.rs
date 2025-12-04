/// This module enable to manage a 'n-list', i.e. a list of n-sized combinations
/// of set cards (of value from 0 to 80):
///     - within which no valid set can be found
///     - with the corresponding list of 'remaining cards' that can be added to 
///       the n-sized combinations without creating a valid set
/// 
/// The methods provided here are used to build such n-lists incrementally,
/// starting from no-set-03 combinations, then no-set-04, no-set-05, etc...
/// 
/// The main function is `build_n+1_set()` which builds the list of all possible
/// no-set-n+1 from a given no-set-n list.

use crate::is_set::*;
use std::cmp::min;
use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct NList {
    pub n: u8,
    pub max_card: usize,
    pub no_set_list: Vec<usize>,
    pub remaining_cards_list: Vec<usize>,
}

impl NList {
    /// return a string representation of the no-set-list
    pub fn to_string(&self) -> String {
        // check there are at least 3 cards in no-set-list
        let nsl_len = self.no_set_list.len();
        if nsl_len < 3 {
            return "invalid".to_string();
        }
        // build no-set-list message
        let mut nsl_msg = "(".to_string();
        for i in &self.no_set_list {
            nsl_msg.push_str(&format!("{:>2}", i));
            if *i < nsl_len {
                nsl_msg.push_str(".");
            }
        }
        nsl_msg.push_str(")");
        // build remaining cards list message
        let rcl_len = self.remaining_cards_list.len();
        let mut rcl_msg = "[".to_string();
        if rcl_len == 0 {
            rcl_msg.push_str("...");
        } else {
            for i in 0..rcl_len  {
                rcl_msg.push_str(&format!("{:>2}", self.remaining_cards_list[i]));
                if i + 1 < rcl_len {
                    rcl_msg.push_str(".");
                }
            }
        }
        rcl_msg.push_str("]");
        // consolidate the whole string
        return format!("{:>2}-list: max={:>2} : {}+{}", self.n, self.max_card, nsl_msg, rcl_msg);
    }

    /// Return a list of n+1-no_set_lists built from the current n-no_set_list
    /// Implementation note:
    ///     - for all card C in the remaining list:
    ///         - create a 'n+1-primary list' with the existing 'primary list' 
    ///           extended with C
    ///      - create a 'cadidate n+1-remaining list' for the 'primary list + C':
    ///          - start from the 'remaining card' list
    ///          - discard any card in this remaining list of a value =< C : this becomes the 'candidate n+1-remaining list'
    ///          - for any card P in the 'primary list':
    ///              - compute the thid card D which form a valid set with C and P
    ///              - check if D is in the 'candidate n+1-remaining list': if yes, remove it from the list
    ///          - if there are not enough cards left in the 'candidate n+1-remaining list' to complement the 'primary list' to 12 cards, it means that the card C is a dead-end: drop it and go to the next card C
    ///          - else you have created a valid n+1-list: store it for later processing, and move the next card C
    ///   - return the list of n+1-no_set_lists created
    pub fn build_new_lists(&self) -> Vec<NList> {
        // we will store the resulting n+1-no_set_lists in new
        let mut n_plus_1_lists = Vec::new();
        // for all card C in the remaining list
        for c in self.remaining_cards_list.iter() {
            // create the n+1-primary list
            let mut n_plus_1_primary_list = self.no_set_list.clone();
            n_plus_1_primary_list.push(*c);
            // create the candidate n+1-remaining list (all cards above c)
            let mut n_plus_1_remaining_list: Vec<usize> = self
                .remaining_cards_list
                .iter()
                .filter(|&&x| x > *c)
                .cloned()
                .collect();
            // for all card P in the primary list, remove from the candidate 
            // remaining list any D card that would form a valid set with C and
            // P
            for p in self.no_set_list.iter() {
                let d = next_to_set(*p, *c);
                n_plus_1_remaining_list.retain(|&x| x != d);
            }
            // check if we have enough cards left in the candidate remaining list
            let cards_needed = 12 - min(self.n as usize + 1, 12);
            if n_plus_1_remaining_list.len() >= cards_needed {
                // we have created a valid n+1-no_set_list: store it
                let n_plus_1_nlist = NList {
                    n: self.n + 1,
                    max_card: *c,
                    no_set_list: n_plus_1_primary_list,
                    remaining_cards_list: n_plus_1_remaining_list,
                };
                n_plus_1_lists.push(n_plus_1_nlist);
            }
        }
        return n_plus_1_lists;
    }
}

/// A structure to hold a list of NList structures, with the ability to save to
/// file the n+1-lists built from a given n-list, per batch of 
/// MAX_NLISTS_PER_FILE, and to load a batch of n-lists from a given file.
#[derive(Serialize, Deserialize)]
pub struct ListOfNlist {
    pub size: u8,                  // # of card in the new nlists
    pub current: Vec<NList>,       // the current n-lists being processed
    pub current_file_count: u64,   // number of the current file being processed
    pub new: Vec<NList>,           // the newly created n+1-lists
    pub new_file_count: u64,       // number of files saved so far
}

impl ListOfNlist {

    /// Max number of n-list saved per file
    pub const MAX_NLISTS_PER_FILE: u64 = 1_000_000;

    /// Creates a new, empty ListOfNlist with n indicating the size of the
    /// current n-lists
    pub fn new(size: u8) -> Self {
        return Self {
            size,
            current: Vec::new(),
            current_file_count: 0,
            new: Vec::new(),
            new_file_count: 0,
        }
    }

    /// Save the current batch of newly computed nlists to file
    ///      - increments the file count
    ///      - clears the new list (to make room for the next batch)
    pub fn save_new_to_file(&mut self) -> bool {
        let filename = filename(self.size, self.new_file_count);
        match save_to_file(&self.new, &filename) {
            true => {
                // the new vector has been saved successfully to file
                self.new_file_count += 1;
                self.new.clear();
                return true;
            },
            false => {
                // error saving to file
                eprintln!("Error saving new list to file {}", filename);
                return false;
            }
        }
    }

    /// Load a batch of current n-lists from a given file and populate the 
    /// current list with it.
    /// 
    /// Typical usage: when the current list has been fully processed and we
    /// want to load the next batch of n-lists (of the same size or not) to 
    /// process.
    /// 
    /// Arguments
    ///     - size: number of card in the current list
    ///     - number of the batch file to load
    /// Returns true on success, false on failure
    pub fn refill_current_from_file(&mut self, current_file_number: u64, size: u8) 
        -> bool {
        let filename = filename(self.size, current_file_number);
        match read_from_file(&filename) {
            Some(vec_nlist) => {
                // successfully read the current vector from file
                self.current = vec_nlist;
                return true;
            },
            None => {
                // error reading from file
                eprintln!("Error loading n-lists from file {}", filename);
                return false;
            }
        }
    }

    /// Processes the current n-lists to build the new lists
    /// Argument: none
    /// Returns: none
    /// and:
    ///     - writes the new n-lists to file in batches of MAX_NLISTS_PER_FILE
    pub fn build_new_lists(&mut self) {

        // do NOT reset the parameters

        // run the algorithm for each list in the current vector 
        for i in 0..self.current.len() {
            // clone the current n-list
            let current_nlist = self.current[i].clone();
            // build the new n-lists from the current n-list
            let new_nlists = current_nlist.build_new_lists();
            // add the newly created n-lists to the new vector
            self.new.extend(new_nlists);
            // check if we have reached the max number of n-lists per file
            if self.new.len() as u64 >= Self::MAX_NLISTS_PER_FILE {
                // save the new n-lists to file
                if !self.save_new_to_file() {
                    eprintln!("Error saving new n-lists to file during build");
                    return; // early exit on error
                }
                println!("   ... saved new batch to {}", filename(self.size, self.new_file_count));
                // increment the file number
                self.new_file_count += 1;
                // reset the new vector
                self.new.clear();
            }
        }
    }

    /// Process all the files for a given size of n-lists
    /// Argument:
    ///     - size: number of card in the n-lists to process
    /// Returns:
    ///     - number of new n-lists created
    /// and
    ///    - writes the new n-lists to file in batches of MAX_NLISTS_PER_FILE
    pub fn process_all_files_for_size_n(&mut self, size: u8) {

        // set all parameters to initial values
        self.size = size + 1;           // we build the n+1-lists
        self.current.clear();
        self.current_file_count = 0;
        self.new.clear();
        self.new_file_count = 0;

        // process all the files for the given size one after the other, until
        // there is no more file to read
        loop {
            let filename = filename(size, self.current_file_count);
            // try to read the current n-lists from file
            match read_from_file(&filename) {
                Some(vec_nlist) => {
                    // successfully read the current vector from file
                    println!("   ... start processing file {}", filename);
                    self.current = vec_nlist;
                    // build the new n-lists from the current n-lists
                    self.build_new_lists();
                    // increment the file number
                    self.current_file_count += 1;
                },
                None => {
                    // no more files to read, exit the loop
                    break;
                }
            }
            //
        }
    }

}

/// Generate a filename for a given n-list size and batch number
pub fn filename(size: u8, batch_number: u64) -> String {
    return format!("nlist_{:02}_batch_{:03}.bin", size, batch_number);
}

/// Saves a list of n-lists to a binary file using bincode serialization
/// 
/// # Arguments
/// * `list_of_nlists` - The list of NList structures to save
/// * `filename` - Path to the output file
/// 
/// # Returns
/// * `Ok(())` on success
/// * `Err` containing the error if serialization or file write fails
pub fn save_to_file(list_of_nlists: &Vec<NList>, filename: &str) -> bool {
    let encoded = bincode::serialize(list_of_nlists);
    if encoded.is_err() {
        eprintln!("Error serializing n-lists for file {}: {}", filename, encoded.err().unwrap());
        return false;
    }
    let result = std::fs::write(filename, encoded.unwrap());
    if result.is_err() {
        eprintln!("Error writing n-lists to file {}: {}", filename, result.err().unwrap());
        return false;
    }
    return true;
}

/// Reads a list of n-lists from a binary file using bincode deserialization
/// 
/// # Arguments
/// * `filename` - Path to the input file
/// 
/// # Returns
/// * `Ok(Vec<NList>)` containing the deserialized list on success
/// * `Err` containing the error if file read or deserialization fails
pub fn read_from_file(filename: &str) -> Option<Vec<NList>> {
    let bytes : Vec<u8>;
    let option_bytes = std::fs::read(filename).ok();
    match option_bytes {
        None => return None,
        Some(b) => bytes = b,
    }
    let option_decoded = bincode::deserialize(&bytes).ok();
    return option_decoded;
}

/// Build the list of all possible no-set-03 combinations, i.e. combinations of 
/// 3 cards within which no valid set can be found, with their corresponding 
/// remaining cards list.
/// 
/// NB:
///     - knowing that we will need to have at least 12 cards on the table 
///       eventually, we limit the max card index to 72 (i.e. one will need to 
///       complement the 3 cards with at least 9 more coards to get to 12).
///     - if we want to focus on the no-set-table with 15 cards, we may stop at
///       max card index 68 (i.e. one will need to complement the 3 cards with
///       at least 12 more cards to get to 15).
///     - if we want to focus on the no-set-table with 18 cards, we may stop at
///       max card index 65 (i.e. one will need to complement the 3 cards with
///       at least 15 more cards to get to 18).
pub fn create_all_03_no_set_lists() -> Vec<NList> {
    // we will store the results in this vector
    let mut no_set_03 = Vec::new();
    // create the no-set-03 combinations (i < 70 to get to at least 12 cards)
    for i in 0..70 {
        for j in (i + 1)..71 {
            for k in (j + 1)..72 {
                // (i,j,k) is a candidate for a no-set-03 combination
                let table = vec![i, j, k];
                if !is_set(i, j, k) {
                    // (i,j,k) is a no-set-03 combination
                    // build a 'remaining list' with all the possible values strictly greater than k
                    let mut remaining_cards: Vec<usize> = (k + 1..81).collect();
                    // remove from this list all cards that would create a set
                    // with any pair of cards in the current table
                    let c1 = next_to_set(i, j);
                    let c2 = next_to_set(i, k);
                    let c3 = next_to_set(j, k);
                    remaining_cards.retain(|&x| x != c1 && x != c2 && x != c3);
                    // store the resulting n-list
                    let nlist = NList {
                        n: 3,
                        max_card: k,
                        no_set_list: table,
                        remaining_cards_list: remaining_cards,
                    };
                    no_set_03.push(nlist);
                }
            }
        }
    }
    return no_set_03;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bincode_roundtrip() {
        // Create test data
        let test_lists = vec![
            NList {
                n: 3,
                max_card: 10,
                no_set_list: vec![0, 5, 10],
                remaining_cards_list: vec![11, 12, 13, 14],
            },
            NList {
                n: 4,
                max_card: 15,
                no_set_list: vec![1, 6, 11, 15],
                remaining_cards_list: vec![16, 17, 18],
            },
        ];

        let filename = "test_roundtrip.bin";
        
        // Save
        save_to_file(&test_lists, filename).expect("Failed to save");
        
        // Load
        let loaded = read_from_file(filename).expect("Failed to load");
        
        // Verify
        assert_eq!(test_lists.len(), loaded.len());
        for (orig, load) in test_lists.iter().zip(loaded.iter()) {
            assert_eq!(orig.n, load.n);
            assert_eq!(orig.max_card, load.max_card);
            assert_eq!(orig.no_set_list, load.no_set_list);
            assert_eq!(orig.remaining_cards_list, load.remaining_cards_list);
        }
        
        // Cleanup
        std::fs::remove_file(filename).ok();
    }
}
