/// Hybrid v0.3.1: Stack-optimized computation with heap-based I/O
/// 
/// This module combines the best of both worlds:
/// - Uses NoSetList (stack arrays) for computation → 4-5× faster
/// - Converts to NList (heap Vecs) for I/O → 6-8× smaller files
/// 
/// Performance characteristics:
/// - Computation: Same speed as v0.3.0 (stack-optimized)
/// - File size: Same as v0.2.2 (~2GB per batch)
/// - File I/O: Same speed as v0.2.2 (small files)
/// - Memory: Moderate (~12-15GB peak during conversion + save)
///
/// Version: 0.3.1 (hybrid approach)

use std::fs::File;

// Rkyv imports for zero-copy serialization
use rkyv::check_archived_root;
use rkyv::Deserialize as RkyvDeserializeTrait;
use memmap2::Mmap;

use separator::Separatable;
use crate::utils::*;
use crate::set::*;
use crate::no_set_list::*;
use crate::nlist::*;

/// Hybrid batch processor: NoSetList for compute, NList for I/O
pub struct ListOfNSLHybrid {
    pub current_size: u8,          // # of cards in the current no-set-lists
    pub current: Vec<NoSetList>,   // current n-lists (stack-based for computation)
    pub current_file_count: u16,   // number of current file being processed
    pub current_list_count: u64,   // number of current n-lists processed so far
    pub new: Vec<NoSetList>,       // newly created n+1-lists (stack-based during compute)
    pub new_file_count: u16,       // number of files saved so far
    pub new_list_count: u64,       // number of new n-lists created so far
    pub base_path: String,         // base directory for saving/loading files
    pub computation_time: f64,     // time spent in core algorithm
    pub file_io_time: f64,         // time spent in file I/O operations
    pub conversion_time: f64,      // time spent converting between formats
}

impl ListOfNSLHybrid {
    /// Creates a new, empty ListOfNSLHybrid with default directory (".")
    pub fn new() -> Self {
        Self {
            current_size: 0,
            current: Vec::new(),
            current_file_count: 0,
            current_list_count: 0,
            new: Vec::new(),
            new_file_count: 0,
            new_list_count: 0,
            base_path: String::from("."),
            computation_time: 0.0,
            file_io_time: 0.0,
            conversion_time: 0.0,
        }
    }
    
    /// Creates a new ListOfNSLHybrid with a custom base path
    pub fn with_path(base_path: &str) -> Self {
        Self {
            current_size: 0,
            current: Vec::new(),
            current_file_count: 0,
            current_list_count: 0,
            new: Vec::new(),
            new_file_count: 0,
            new_list_count: 0,
            base_path: String::from(base_path),
            computation_time: 0.0,
            file_io_time: 0.0,
            conversion_time: 0.0,
        }
    }
    
    /// Build all possible no-set-03 combinations using stack allocation
    pub fn create_seed_lists(&mut self) {
        // Start timing
        let start_time = std::time::Instant::now();
        
        // Initialize fields
        self.current_size = 3;
        self.current.clear();
        self.current_file_count = 0;
        self.current_list_count = 0;
        self.new.clear();
        self.new_file_count = 0;
        self.new_list_count = 0;
        
        // Create no-set-03 combinations (i < 70 to reach at least 12 cards)
        for i in 0..70 {
            for j in (i + 1)..71 {
                for k in (j + 1)..72 {
                    // Check if (i,j,k) forms a set
                    if !is_set(i, j, k) {
                        // Build seed list on stack
                        let mut no_set_array = [0usize; 18];
                        no_set_array[0] = i;
                        no_set_array[1] = j;
                        no_set_array[2] = k;
                        
                        // Remaining cards: stack array with filtering
                        let mut remaining_array = [0usize; 78];
                        let mut remaining_len = 0u8;
                        
                        // Add all cards > k
                        for card in (k + 1)..81 {
                            remaining_array[remaining_len as usize] = card;
                            remaining_len += 1;
                        }
                        
                        // Remove forbidden cards
                        let forbidden = [
                            next_to_set(i, j),
                            next_to_set(i, k),
                            next_to_set(j, k),
                        ];
                        
                        for &f in &forbidden {
                            let mut idx = 0u8;
                            while idx < remaining_len {
                                if remaining_array[idx as usize] == f {
                                    // Shift left to remove
                                    for m in idx..remaining_len - 1 {
                                        remaining_array[m as usize] = remaining_array[(m + 1) as usize];
                                    }
                                    remaining_len -= 1;
                                    break;
                                }
                                idx += 1;
                            }
                        }
                        
                        // Create NoSetList (stack-allocated)
                        let nsl = NoSetList {
                            size: 3,
                            max_card: k,
                            no_set_list: no_set_array,
                            no_set_list_len: 3,
                            remaining_cards_list: remaining_array,
                            remaining_cards_list_len: remaining_len,
                        };
                        
                        self.current.push(nsl);
                    }
                }
            }
        }
        
        self.current_list_count = self.current.len() as u64;
        
        // Convert to NList and save (hybrid I/O)
        let conv_start = std::time::Instant::now();
        let nlists: Vec<NList> = self.current.iter().map(|nsl| nsl.to_nlist()).collect();
        self.conversion_time = conv_start.elapsed().as_secs_f64();
        
        // Clone to fresh Vecs to eliminate capacity bloat
        let compacted: Vec<NList> = nlists.iter().map(|nlist| NList {
            n: nlist.n,
            max_card: nlist.max_card,
            no_set_list: nlist.no_set_list.iter().copied().collect(),
            remaining_cards_list: nlist.remaining_cards_list.iter().copied().collect(),
        }).collect();
        
        let file = filename(&self.base_path, 3, 0);
        
        let io_start = std::time::Instant::now();
        match save_to_file_nlist(&compacted, &file) {
            true => debug_print(&format!("create_seed_lists: saved {} seed lists to {}", 
                self.current_list_count, file)),
            false => debug_print(&format!("create_seed_lists: Error saving seed lists to {}", 
                file)),
        }
        self.file_io_time = io_start.elapsed().as_secs_f64();
        
        // Report completion with timing
        let elapsed = start_time.elapsed();
        let elapsed_secs = elapsed.as_secs_f64();
        created_a_total_of(self.current_list_count, 3, "v0.3.1", elapsed_secs);
        
        // Clear current list to make room for processing
        self.current.clear();
        self.current_list_count = 0;
    }
    
    /// Load a batch of current n-lists from file (reads NList, converts to NoSetList)
    fn refill_current_from_file(&mut self) -> bool {
        let filename = filename(&self.base_path, self.current_size, self.current_file_count);
        
        // Time the file read operation
        let io_start = std::time::Instant::now();
        
        let result = read_from_file_nlist(&filename);
        self.file_io_time += io_start.elapsed().as_secs_f64();
        
        match result {
            Some(vec_nlist) => {
                // Convert from NList to NoSetList for fast computation
                let conv_start = std::time::Instant::now();
                let vec_nsl: Vec<NoSetList> = vec_nlist.iter()
                    .map(|nl| NoSetList::from_nlist(nl))
                    .collect();
                self.conversion_time += conv_start.elapsed().as_secs_f64();
                
                let add_len = vec_nsl.len();
                self.current.extend(vec_nsl);
                self.current_list_count += add_len as u64;
                self.current_file_count += 1;
                debug_print(&format!("refill_current_from_file: added {} n-lists from {} \
                    (total: {}, files: {})", add_len, filename, self.current_list_count, 
                    self.current_file_count));
                true
            }
            None => {
                debug_print(&format!("refill_current_from_file: Error loading from {}", 
                    filename));
                false
            }
        }
    }
    
    /// Save current batch (converts NoSetList to NList for compact storage)
    fn save_new_to_file(&mut self) -> bool {
        let file = filename(&self.base_path, self.current_size + 1, self.new_file_count);
        let additional_new = self.new.len() as u64;
        
        // Convert to NList for compact serialization
        let conv_start = std::time::Instant::now();
        let nlists: Vec<NList> = self.new.iter().map(|nsl| nsl.to_nlist()).collect();
        self.conversion_time += conv_start.elapsed().as_secs_f64();
        
        // Clone to fresh Vecs to eliminate capacity bloat
        let compacted: Vec<NList> = nlists.iter().map(|nlist| NList {
            n: nlist.n,
            max_card: nlist.max_card,
            no_set_list: nlist.no_set_list.iter().copied().collect(),
            remaining_cards_list: nlist.remaining_cards_list.iter().copied().collect(),
        }).collect();
        
        // Time the file write operation
        let io_start = std::time::Instant::now();
        
        match save_to_file_nlist(&compacted, &file) {
            true => {
                self.file_io_time += io_start.elapsed().as_secs_f64();
                
                self.new_list_count += additional_new;
                self.new_file_count += 1;
                self.new.clear();
                test_print(&format!("   ... saved {} n-lists to {}", 
                    additional_new, file));
                true
            }
            false => {
                self.file_io_time += io_start.elapsed().as_secs_f64();
                debug_print(&format!("save_new_to_file: Error saving to {}", file));
                false
            }
        }
    }
    
    /// Process one batch using stack-optimized computation
    fn process_one_file_of_current_size_n(&mut self, max: &u64) {
        debug_print(&format!("process_one_file_of_current_size_n: Processing file {} \
            of no-set-{:02} ({} lists)", self.current_file_count, self.current_size, 
            self.current.len()));
        
        let len = self.current.len() as u64;
        let mut i = 0u64;
        
        while !self.current.is_empty() {
            debug_print_noln(&format!("{:>5} ", len - i));
            
            // Pop current n-list
            let current_nsl = self.current.pop().unwrap();
            
            // Time the core computation (STACK-OPTIMIZED)
            let comp_start = std::time::Instant::now();
            let new_nsls = current_nsl.build_higher_nsl();
            self.computation_time += comp_start.elapsed().as_secs_f64();
            
            debug_print_noln(&format!("-> +{:>5} new - ", new_nsls.len()));
            
            // Add to new vector (still NoSetList for now)
            self.new.extend(new_nsls);
            
            if i % 4 == 0 || i + 1 == len {
                debug_print(&format!(" - {:>8}", self.new.len()));
            }
            
            // Check if we need to save
            if self.new.len() as u64 >= *max {
                if self.save_new_to_file() {
                    self.new.clear();
                } else {
                    debug_print("process_one_file_of_current_size_n: Error saving batch");
                }
            }
            
            i += 1;
        }
    }
    
    /// Process all files for a given size
    pub fn process_all_files_of_current_size_n(&mut self, current_size: u8, max: &u64) -> u64 {
        if current_size < 3 {
            debug_print("process_all_files_of_current_size_n: size must be >= 3");
            return 0;
        }
        
        debug_print(&format!("process_all_files_of_current_size_n: start processing \
            no-set-{:02}", current_size));
        
        // Start timing
        let start_time = std::time::Instant::now();
        
        // Reset timing counters
        self.computation_time = 0.0;
        self.file_io_time = 0.0;
        self.conversion_time = 0.0;
        
        // Initialize
        self.current_size = current_size;
        self.current.clear();
        self.current_file_count = 0;
        self.new.clear();
        self.new_file_count = 0;
        
        // Process all files
        loop {
            debug_print(&format!("process_all_files_of_current_size_n: loading file {} \
                for size {:02}", self.current_file_count, self.current_size));
            
            let loaded = self.refill_current_from_file();
            if loaded {
                debug_print(&format!("process_all_files_of_current_size_n: loaded {} n-lists", 
                    self.current.len()));
                self.process_one_file_of_current_size_n(max);
            } else {
                debug_print(&format!("process_all_files_of_current_size_n: no more files \
                    for size {:02}", self.current_size));
                break;
            }
        }
        
        // Save remaining n-lists
        if !self.new.is_empty() {
            debug_print(&format!("process_all_files_of_current_size_n: saving final batch \
                of {}", self.new.len()));
            if self.save_new_to_file() {
                debug_print("process_all_files_of_current_size_n: final batch saved");
            } else {
                debug_print("process_all_files_of_current_size_n: Error saving final batch");
            }
        }
        
        let elapsed = start_time.elapsed();
        let elapsed_secs = elapsed.as_secs_f64();
        let overhead = elapsed_secs - self.computation_time - self.file_io_time - self.conversion_time;
        
        debug_print(&format!("process_all_files_of_current_size_n: Finished \
            processing size {:02}", self.current_size));
        
        // Report total with breakdown
        created_a_total_of(self.new_list_count, self.current_size + 1, 
            "v0.3.1", elapsed_secs);
        test_print(&format!("   ... timing breakdown: computation {:.2}s \
            ({:.1}%), file I/O {:.2}s ({:.1}%), conversion {:.2}s ({:.1}%), \
            overhead {:.2}s ({:.1}%)",
            self.computation_time, (self.computation_time / elapsed_secs * 100.0),
            self.file_io_time, (self.file_io_time / elapsed_secs * 100.0),
            self.conversion_time, (self.conversion_time / elapsed_secs * 100.0),
            overhead, (overhead / elapsed_secs * 100.0)));
        
        self.new_list_count
    }
}

impl Default for ListOfNSLHybrid {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to print large numbers with thousand separators and timing info
pub fn created_a_total_of(nb: u64, size: u8, version: &str, elapsed_secs: f64) {
    let hours = (elapsed_secs / 3600.0) as u64;
    let minutes = ((elapsed_secs % 3600.0) / 60.0) as u64;
    let seconds = (elapsed_secs % 60.0) as u64;
    
    test_print(&format!("   ... {} created a total of {:>15} no-set-{:02} lists in {:>10.2} seconds ({:02}h{:02}m{:02}s)", 
            version, nb.separated_string(), size, elapsed_secs, hours, minutes, seconds));
}

/// Generate filename for hybrid files (.rkyv extension, NList format)
fn filename(base_path: &str, size: u8, batch_number: u16) -> String {
    use std::path::Path;
    // Use .rkyv extension with v31 prefix to distinguish from v0.2.2
    let filename = format!("nlist_v31_{:02}_batch_{:03}.rkyv", size, batch_number);
    let path = Path::new(base_path).join(filename);
    path.to_string_lossy().to_string()
}

/// Save NList vector to file using rkyv
fn save_to_file_nlist(list: &Vec<NList>, filename: &str) -> bool {
    debug_print(&format!("save_to_file_nlist: Serializing {} n-lists to {} using rkyv", 
        list.len(), filename));
    
    // Serialize to memory buffer using rkyv
    let bytes = match rkyv::to_bytes::<_, 256>(list) {
        Ok(b) => b,
        Err(e) => {
            debug_print(&format!("save_to_file_nlist: Error serializing: {}", e));
            return false;
        }
    };
    
    let bytes_len = bytes.len();
    
    // Write to file
    match std::fs::write(filename, bytes) {
        Ok(_) => {
            debug_print(&format!("save_to_file_nlist: Saved {} bytes to {}", bytes_len, filename));
            true
        }
        Err(e) => {
            debug_print(&format!("save_to_file_nlist: Error writing {}: {}", filename, e));
            false
        }
    }
}

/// Read NList vector from file using rkyv with memory mapping
fn read_from_file_nlist(filename: &str) -> Option<Vec<NList>> {
    debug_print(&format!("read_from_file_nlist: Loading n-lists from {} using rkyv", filename));
    
    // Open file
    let file = match File::open(filename) {
        Ok(f) => f,
        Err(e) => {
            debug_print(&format!("read_from_file_nlist: Error opening {}: {}", filename, e));
            return None;
        }
    };
    
    // Memory-map the file for zero-copy access
    let mmap = unsafe {
        match Mmap::map(&file) {
            Ok(m) => m,
            Err(e) => {
                debug_print(&format!("read_from_file_nlist: Error mapping {}: {}", filename, e));
                return None;
            }
        }
    };
    
    debug_print(&format!("read_from_file_nlist: mapped {} bytes from {}", mmap.len(), filename));
    
    // Validate and deserialize
    match check_archived_root::<Vec<NList>>(&mmap) {
        Ok(archived_vec) => {
            let deserialized: Vec<NList> = archived_vec
                .deserialize(&mut rkyv::Infallible)
                .expect("Deserialization should not fail after validation");
            
            debug_print(&format!("read_from_file_nlist: deserialized {} n-lists", 
                deserialized.len()));
            Some(deserialized)
        }
        Err(e) => {
            debug_print(&format!("read_from_file_nlist: Validation error for {}: {:?}", 
                filename, e));
            None
        }
    }
}
