/// This module enable to manage lists of 'n-list', i.e. a vector of NList structures
/// 
/// Each NList structure represents a combination of n cards, represented by
/// their indices in the full deck of 81 cards (from 0 to 80), such that:
///     - within which no valid set can be found
///     - with the corresponding list of 'remaining cards' that can be added to 
///       the n-sized combinations without creating a valid set
/// 
/// The methods provided here are used to build such lists of NLists 
/// incrementally, starting from no-set-03 combinations, then no-set-04, 
/// no-set-05, etc...
/// 
/// The main function is `build_n+1_set()` which builds the list of all possible
/// no-set-n+1 from a given no-set-n list.

use std::cmp::min;
use serde::{Serialize, Deserialize};
use separator::Separatable;
use crate::utils::*;
use crate::set::*;
use crate::nlist::*;

/// A structure to hold a list of NList structures, with the ability to save to
/// file the n+1-lists built from a given n-list, per batch of 
/// MAX_NLISTS_PER_FILE, and to load a batch of n-lists from a given file.
#[derive(Serialize, Deserialize)]
pub struct ListOfNlist {
    pub current_size: u8,          // # of card in the current nlists
    pub current: Vec<NList>,       // the current n-lists being processed
    pub current_file_count: u16,   // number of the current file being processed
    pub current_list_count: u64,   // number of current n-lists processed so far
    pub new: Vec<NList>,           // the newly created n+1-lists
    pub new_file_count: u16,       // number of files saved so far
    pub new_list_count: u64,       // number of new n-lists created so far
    #[serde(skip)]
    pub base_path: String,         // base directory for saving/loading files
}

impl ListOfNlist {

    /// Creates a new, empty ListOfNlist with n indicating the size of the
    /// current n-lists
    /// 
    /// # Arguments
    /// * `base_path` - Optional base directory path for saving/loading files.
    ///                 If None, uses current directory (".").
    ///                 Examples:
    ///                 - Windows: r"T:\data\funny_set_exploration"
    ///                 - Linux: "/mnt/nas/data/funny_set_exploration"
    pub fn new() -> Self {
        return Self {
            current_size: 0,
            current: Vec::new(),
            current_file_count: 0,
            current_list_count: 0,
            new: Vec::new(),
            new_file_count: 0,
            new_list_count: 0,
            base_path: String::from("."),
        }
    }

    /// Creates a new ListOfNlist with a custom base path
    /// 
    /// # Arguments
    /// * `base_path` - Base directory path for saving/loading files
    ///                 Examples:
    ///                 - Windows: r"T:\data\funny_set_exploration"
    ///                 - Linux: "/mnt/nas/data/funny_set_exploration"
    pub fn with_path(base_path: &str) -> Self {
        return Self {
            current_size: 0,
            current: Vec::new(),
            current_file_count: 0,
            current_list_count: 0,
            new: Vec::new(),
            new_file_count: 0,
            new_list_count: 0,
            base_path: String::from(base_path),
        }
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
    pub fn create_seed_lists(&mut self) {
        // set the fields with initial values
        self.current_size = 3;          // we handle list of 3 cards
        self.current.clear();           // clear existing current n-lists
        self.current_file_count = 0;    // reset current file count
        self.current_list_count = 0;    // reset current list count
        self.new.clear();               // clear existing new n-lists
        self.new_file_count = 0;        // reset new file count
        self.new_list_count = 0;        // reset new list count
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
                        self.current.push(nlist);
                    }
                }
            }
        }
        self.current_list_count = self.current.len() as u64;

        // done with creating all seed-lists: save them to file
        created_a_total_of(self.current_list_count, 3);
        let file = filename(&self.base_path, 3, 0);
        match save_to_file(&self.current, &file) {
            true => debug_print(&format!("create_seed_lists:   ... saved {} seed \
                        lists to {}", self.current_list_count, file)),
            false => debug_print(&format!("create_seed_lists: Error saving \
                        seed lists to file {}", file)),
        }
        // now clear the current list to make room for processing higher n-lists
        self.current.clear();
        self.current_list_count = 0;
    }

    /// Load a batch of current n-lists from a given file and populate the 
    /// current list with it.
    /// 
    /// Typical usage: when the current list has been fully processed and we
    /// want to load the next batch of n-lists (of the same size or not) to 
    /// continue the process.
    /// 
    /// Arguments
    ///     - size: number of card in the current list
    ///     - number of the batch file to load
    /// Returns true on success, false on failure
    fn refill_current_from_file(&mut self) -> bool {
        // build the right file name
        let filename = filename(&self.base_path, self.current_size, self.current_file_count);
        // try reading the file
        match read_from_file(&filename) {
            Some(vec_nlist) => {
                // successfully read the current vector from file: add the 
                // n-lists to the current vector
                let add_len = vec_nlist.len();
                self.current.extend(vec_nlist);
                self.current_list_count += self.current.len() as u64;
                self.current_file_count += 1;
                debug_print(&format!("refill_current_from_file:   ... added {} \
                    current n-lists from {} => total current n-list now {}, \
                    current file count = {}, new file count = {}", add_len, 
                    filename, self.current_list_count, self.current_file_count, 
                    self.new_file_count));
                return true;
            },
            None => {
                // error reading from file
                debug_print(&format!("refill_current_from_file: Error loading \
                    n-lists from file {}", filename));
                return false;
            }
        }
    }

    /// Save the current batch of newly computed nlists to file
    ///      - increments the file count
    ///      - clears the new list (to make room for the next batch)
    fn save_new_to_file(&mut self) -> bool {
        // build the file name
        let file = filename(&self.base_path, self.current_size+1, 
            self.new_file_count);
        // get the number of new n-lists to be saved
        let additional_new = self.new.len() as u64;

        // try saving the new vector to file
        match save_to_file(&self.new, &file) {
            true => {
                // the new vector has been saved successfully to file
                self.new_list_count += additional_new;
                self.new_file_count += 1;
                self.new.clear();
                test_print(&format!("   ... save_new_to_file: saved new batch \
                    of {} n-lists to {}", additional_new, file));
                return true;
            },
            false => {
                // error saving to file
                debug_print(&format!("save_new_to_file: Error saving new list \
                    to file {}", file));
                return false;
            }
        }
    }

    /// Processes the current n-lists to build the new lists
    /// Argument: none
    /// Returns: none
    /// and:
    ///     - writes the new n-lists to file in batches of MAX_NLISTS_PER_FILE
    fn process_one_file_of_current_size_n(&mut self, max: &u64) {

        // do NOT reset the parameters
        debug_print(&format!("process_one_file_of_current_size_n: Processing \
            file {} of current no-set-{:02} => will process {} lists to build no-set-{:02} lists", 
            self.current_file_count, self.current_size, self.current.len(),
            self.current_size+1));
        // run the algorithm for each list in the current vector
        let len = self.current.len() as u64;
        let mut i: u64 = 0; 
        while !self.current.is_empty() {
            debug_print_noln(&format!("{:>5} ", len - i));
            // pop the first current n-list from the vector
            let current_nlist = self.current.pop().unwrap();
            // build the new n-lists from the current n-list
            let new_nlists = current_nlist.build_higher_nlists();
            debug_print_noln(&format!("-> +{:>5} new - ", new_nlists.len()));
            // add the newly created n-lists to the new vector
            self.new.extend(new_nlists);
            if i % 4 == 0 || i + 1 == len {
                debug_print(&format!(" - {:>8}", self.new.len()));
            }
            // check if we have reached the max number of n-lists per file
            if self.new.len() as u64 >= *max {
                // save the new n-lists to file
                let saved_ok = self.save_new_to_file();
                if saved_ok {
                    // the new n-lists were saved to file => reset the new vector
                    self.new.clear();
                    // no other change needed
                } else {
                    // error saving to file
                    debug_print(&format!("process_one_file_of_current_size_n: Error saving new n-lists to file during build"));
                    // no early exit on error, let's see...
                }
            }
            i += 1;
        }
    }

    /// Process all the files for a given size of n-lists
    /// Argument:
    ///     - size: number of card in the n-lists to process
    /// Returns:
    ///     - number of new n-lists created
    /// and
    ///    - writes the new n-lists to file in batches of MAX_NLISTS_PER_FILE
    pub fn process_all_files_of_current_size_n(&mut self, current_size: u8, 
        max: &u64) -> u64 {
        // eligible if size >= 3
        if current_size < 3 {
            debug_print("process_all_files_of_current_size_n: size must be >= 3");
            return 0;
        }
        debug_print(&format!("process_all_files_of_current_size_n: start processing files with no-set size {:02}", current_size));

        // set all parameters to initial values
        self.current_size = current_size; // we process lists of size n-1 to build lists of size n
        self.current.clear();
        self.current_file_count = 0;
        self.new.clear();
        self.new_file_count = 0;

        // process all the files for the given size one after the other, until
        // there is no more file to read
        loop {
            // load the next file of current n-lists
            debug_print(&format!("process_all_files_of_current_size_n: current = {} nlists => \
                will load file number {} for size {:02}", self.current.len(),
                self.current_file_count, self.current_size));
            let loaded = self.refill_current_from_file();
            if loaded {
                // successfully loaded a new batch of current n-lists
                debug_print(&format!("process_all_files_of_current_size_n:   ... loaded {} current n-lists", 
                    self.current.len()));
                self.process_one_file_of_current_size_n(max);
            } else {
                // error loading the next file: we are done
                debug_print(&format!("process_all_files_of_current_size_n:   ... no more file to load for size {:02}", 
                    self.current_size));
                break;
            }
        }
        // save any remaining new n-lists to file
        let remaining_new = self.new.len() as u64;
        if remaining_new > 0 {
            debug_print(&format!("process_all_files_of_current_size_n:   \
                ... will save final batch of {} new lists to {}", 
                self.new.len(),
                filename(&self.base_path, self.current_size+1, self.new_file_count)));
            if self.save_new_to_file() {
                debug_print("process_all_files_of_current_size_n:   ... final batch saved successfully");
            } else {
                debug_print("process_all_files_of_current_size_n: Error saving final batch of new n-lists to file");
            }
        }
        // this is done
        debug_print(&format!("process_all_files_of_current_size_n: Finished processing all files for size {:02}", 
            self.current_size));
        return self.new_list_count;
    }
}

/// helper to properly print a large number of n-lists
pub fn created_a_total_of(nb: u64, size: u8) {
    test_print(&format!("   ... created a total of {:>15} no-set-{:02} lists", 
            nb.separated_string(), size));
}

/// Generate a filename for a given n-list size and batch number
/// 
/// # Arguments
/// * `base_path` - Base directory path (e.g., ".", "T:\\data\\funny_set_exploration", "/mnt/nas/data")
/// * `size` - Size of the n-list
/// * `batch_number` - Batch number
/// 
/// # Returns
/// Full path to the file
fn filename(base_path: &str, size: u8, batch_number: u16) -> String {
    use std::path::Path;
    let filename = format!("nlist_{:02}_batch_{:03}.bin", size, batch_number);
    let path = Path::new(base_path).join(filename);
    return path.to_string_lossy().to_string();
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
fn save_to_file(list_of_nlists: &Vec<NList>, filename: &str) -> bool {
    let encoded = bincode::serialize(list_of_nlists);
    if encoded.is_err() {
        debug_print(&format!("save_to_file: Error serializing n-lists for file \
            {}: {}", filename, encoded.err().unwrap()));
        return false;
    }
    let result = std::fs::write(filename, 
        encoded.unwrap());
    if result.is_err() {
        debug_print(&format!("save_to_file: Error writing n-lists to file {}: \
            {}", filename, result.err().unwrap()));
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
fn read_from_file(filename: &str) -> Option<Vec<NList>> {
    debug_print(&format!("read_from_file: Loading n-lists from file {}", 
        filename));

    let option_bytes = std::fs::read(filename).ok();
    match option_bytes {
        None => {
            debug_print(&format!("read_from_file: Error reading n-lists from \
                file {}", filename));
            return None;
        }
        Some(b) => {
            debug_print(&format!("read_from_file:   ... read {} bytes from \
                file {}", b.len(), filename));
            let option_decoded = bincode::deserialize(&b).ok();
            return option_decoded;
        }
    }
}

