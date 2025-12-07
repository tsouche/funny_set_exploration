/// Version 0.4.0: Hybrid stack-optimized computation with heap-based I/O + Restart capability
/// 
/// This implementation combines the best of both worlds:
/// - Uses NoSetList (stack arrays) for computation → 4-5× faster
/// - Converts to NoSetListSerialized (heap Vecs) for I/O → compact 2GB files
/// 
/// Performance characteristics:
/// - Computation: Same speed as v0.3.0 (stack-optimized)
/// - File size: ~2GB per 20M batch (compact with size_32 rkyv)
/// - Memory: Moderate (~12-15GB peak during conversion + save)
///
/// This is the only active version of the project.

use std::fs::File;

// Rkyv imports for zero-copy serialization
use rkyv::check_archived_root;
use rkyv::Deserialize as RkyvDeserializeTrait;
use memmap2::Mmap;

use separator::Separatable;
use crate::utils::*;
use crate::set::*;
use crate::no_set_list::*;

/// Batch processor: NoSetList for compute, NoSetListSerialized for I/O
pub struct ListOfNSL {
    pub current_size: u8,              // # of cards in the current no-set-lists
    pub current: Vec<NoSetList>,       // current n-lists (stack-based for computation)
    pub current_file_batch: u16,       // Current input file batch number
    pub current_file_list_count: u64,  // Lists loaded from current input file
    pub current_total_list_count: u64, // Total lists processed across all input files
    pub new: Vec<NoSetList>,           // newly created n+1-lists (stack-based during compute)
    pub new_output_batch: u16,         // Current output file batch for this input
    pub new_file_list_count: u64,      // Lists saved to current output file
    pub new_total_list_count: u64,     // Total lists created for target size
    pub base_path: String,             // base directory for saving/loading files
    pub computation_time: f64,         // time spent in core algorithm
    pub file_io_time: f64,             // time spent in file I/O operations
    pub conversion_time: f64,          // time spent converting between formats
}

impl ListOfNSL {
    /// Creates a new, empty ListOfNSL with default directory (".")
    pub fn new() -> Self {
        Self {
            current_size: 0,
            current: Vec::new(),
            current_file_batch: 0,
            current_file_list_count: 0,
            current_total_list_count: 0,
            new: Vec::new(),
            new_output_batch: 0,
            new_file_list_count: 0,
            new_total_list_count: 0,
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
            current_file_batch: 0,
            current_file_list_count: 0,
            current_total_list_count: 0,
            new: Vec::new(),
            new_output_batch: 0,
            new_file_list_count: 0,
            new_total_list_count: 0,
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
        self.current_file_batch = 0;
        self.current_file_list_count = 0;
        self.current_total_list_count = 0;
        self.new.clear();
        self.new_output_batch = 0;
        self.new_file_list_count = 0;
        self.new_total_list_count = 0;
        
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
        
        self.current_file_list_count = self.current.len() as u64;
        
        // Convert to NoSetListSerialized and save (hybrid I/O)
        let conv_start = std::time::Instant::now();
        let nlists: Vec<NoSetListSerialized> = self.current.iter().map(|nsl| nsl.to_serialized()).collect();
        self.conversion_time += conv_start.elapsed().as_secs_f64();
        
        // Clone to fresh Vecs to eliminate capacity bloat
        let compacted: Vec<NoSetListSerialized> = nlists.iter().map(|nlist| NoSetListSerialized {
            n: nlist.n,
            max_card: nlist.max_card,
            no_set_list: nlist.no_set_list.iter().copied().collect(),
            remaining_cards_list: nlist.remaining_cards_list.iter().copied().collect(),
        }).collect();
        
        let file = output_filename(&self.base_path, 0, 0, 3, 0);
        
        let io_start = std::time::Instant::now();
        match save_to_file_serialized(&compacted, &file) {
            true => debug_print(&format!("create_seed_lists: saved {} seed lists to {}", 
                self.current_file_list_count, file)),
            false => debug_print(&format!("create_seed_lists: Error saving seed lists to {}", 
                file)),
        }
        self.file_io_time = io_start.elapsed().as_secs_f64();
        
        // Report completion with timing
        let elapsed = start_time.elapsed();
        let elapsed_secs = elapsed.as_secs_f64();
        created_a_total_of(self.current_file_list_count, 3, elapsed_secs);
        
        // Clear current list to make room for processing
        self.current.clear();
        self.current_file_list_count = 0;
    }
    
    /// Load a batch of current n-lists from file (reads NoSetListSerialized, converts to NoSetList)
    /// Reads output files from previous processing step that target current_size
    fn refill_current_from_file(&mut self) -> bool {
        // Find input file: any file that was output to create current_size at current_file_batch
        let filename = match find_input_filename(&self.base_path, self.current_size, self.current_file_batch) {
            Some(f) => f,
            None => {
                debug_print(&format!("refill_current_from_file: No file found for size {:02} batch {:03}",
                    self.current_size, self.current_file_batch));
                return false;
            }
        };
        
        // Time the file read operation
        let io_start = std::time::Instant::now();
        
        let result = read_from_file_serialized(&filename);
        self.file_io_time += io_start.elapsed().as_secs_f64();
        
        match result {
            Some(vec_nlist) => {
                // Convert from NoSetListSerialized to NoSetList for fast computation
                let conv_start = std::time::Instant::now();
                let vec_nsl: Vec<NoSetList> = vec_nlist.iter()
                    .map(|nl| NoSetList::from_serialized(nl))
                    .collect();
                self.conversion_time += conv_start.elapsed().as_secs_f64();
                test_print(&format!("   ... loaded  {:>10} no-set-lists from {}", 
                    vec_nsl.len().separated_string(), filename));
                let add_len = vec_nsl.len();
                self.current.extend(vec_nsl);
                self.current_file_list_count = add_len as u64;
                self.current_total_list_count += add_len as u64;
                debug_print(&format!("refill_current_from_file: added {} n-lists from {} \
                    (file: {}, cumulative: {})", add_len, filename, 
                    self.current_file_list_count, self.current_total_list_count));
                true
            }
            None => {
                debug_print(&format!("refill_current_from_file: Error loading from {}", 
                    filename));
                false
            }
        }
    }
    
    /// Save current batch (converts NoSetList to NoSetListSerialized for compact storage)
    fn save_new_to_file(&mut self) -> bool {
        let file = output_filename(
            &self.base_path, 
            self.current_size, 
            self.current_file_batch,
            self.current_size + 1, 
            self.new_output_batch
        );
        let additional_new = self.new.len() as u64;
        
        // Convert to NoSetListSerialized for compact serialization
        let conv_start = std::time::Instant::now();
        let nlists: Vec<NoSetListSerialized> = self.new.iter().map(|nsl| nsl.to_serialized()).collect();
        self.conversion_time += conv_start.elapsed().as_secs_f64();
        
        // Clone to fresh Vecs to eliminate capacity bloat
        let compacted: Vec<NoSetListSerialized> = nlists.iter().map(|nlist| NoSetListSerialized {
            n: nlist.n,
            max_card: nlist.max_card,
            no_set_list: nlist.no_set_list.iter().copied().collect(),
            remaining_cards_list: nlist.remaining_cards_list.iter().copied().collect(),
        }).collect();
        
        // Time the file write operation
        let io_start = std::time::Instant::now();
        
        match save_to_file_serialized(&compacted, &file) {
            true => {
                self.file_io_time += io_start.elapsed().as_secs_f64();
                
                self.new_total_list_count += additional_new;
                self.new_output_batch += 1;
                self.new.clear();
                test_print(&format!("   ... saved   {:>10} no-set-lists  to  {}", 
                    additional_new.separated_string(), file));
                true
            }
            false => {
                self.file_io_time += io_start.elapsed().as_secs_f64();
                debug_print(&format!("save_new_to_file: Error saving to {}", file));
                false
            }
        }
    }
    
    /// Process one input file using stack-optimized computation
    /// Creates output files with modular naming and closes output when input exhausted
    fn process_one_file_of_current_size_n(&mut self, max: &u64) -> u64 {
        debug_print(&format!("process_one_file_of_current_size_n: Processing batch {} \
            of no-set-{:02} ({} lists)", self.current_file_batch, self.current_size, 
            self.current.len()));
        
        // Reset output counters for this input file
        self.new_output_batch = 0;
        let file_new_count_start = self.new_total_list_count;
        
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
                if !self.save_new_to_file() {
                    debug_print("process_one_file_of_current_size_n: Error saving batch");
                }
            }
            
            i += 1;
        }
        
        // Save any remaining lists from this input file (even if < max)
        if !self.new.is_empty() {
            debug_print(&format!("process_one_file_of_current_size_n: saving final batch of {}", 
                self.new.len()));
            if !self.save_new_to_file() {
                debug_print("process_one_file_of_current_size_n: Error saving final batch");
            }
        }
        
        // Calculate and log this file's statistics
        let file_new_total = self.new_total_list_count - file_new_count_start;
        test_print(&format!("   ... processed {} input lists, created {} new lists from batch {:03}",
            self.current_file_list_count.separated_string(),
            file_new_total.separated_string(),
            self.current_file_batch));
        
        file_new_total
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
        
        // Initialize for this size
        self.current_size = current_size;
        self.current.clear();
        self.current_file_batch = 0;
        self.current_file_list_count = 0;
        self.current_total_list_count = 0;
        self.new.clear();
        self.new_output_batch = 0;
        self.new_file_list_count = 0;
        self.new_total_list_count = 0;
        
        // Process all input files for this size
        loop {
            debug_print(&format!("process_all_files_of_current_size_n: loading batch {} \
                for size {:02}", self.current_file_batch, self.current_size));
            
            let loaded = self.refill_current_from_file();
            if loaded {
                debug_print(&format!("process_all_files_of_current_size_n: loaded {} n-lists", 
                    self.current.len()));
                self.process_one_file_of_current_size_n(max);
                self.current_file_batch += 1;
            } else {
                debug_print(&format!("process_all_files_of_current_size_n: no more files \
                    for size {:02}", self.current_size));
                break;
            }
        }
        
        let elapsed = start_time.elapsed();
        let elapsed_secs = elapsed.as_secs_f64();
        let overhead = elapsed_secs - self.computation_time - self.file_io_time - self.conversion_time;
        
        debug_print(&format!("process_all_files_of_current_size_n: Finished \
            processing size {:02}", self.current_size));
        
        // Report total with breakdown
        created_a_total_of(self.new_total_list_count, self.current_size + 1, elapsed_secs);
        test_print(&format!("   ... timing breakdown: computation {:.2}s \
            ({:.1}%), file I/O {:.2}s ({:.1}%), conversion {:.2}s ({:.1}%), \
            overhead {:.2}s ({:.1}%)",
            self.computation_time, (self.computation_time / elapsed_secs * 100.0),
            self.file_io_time, (self.file_io_time / elapsed_secs * 100.0),
            self.conversion_time, (self.conversion_time / elapsed_secs * 100.0),
            overhead, (overhead / elapsed_secs * 100.0)));
        
        self.new_total_list_count
    }
    
    /// Process files starting from a specific batch number (for restart capability)
    /// Used to resume processing after interruption
    pub fn process_from_batch(&mut self, current_size: u8, start_batch: u16, max: &u64) -> u64 {
        if current_size < 3 {
            debug_print("process_from_batch: size must be >= 3");
            return 0;
        }
        
        debug_print(&format!("process_from_batch: start processing no-set-{:02} from batch {}", 
            current_size, start_batch));
        
        // Start timing
        let start_time = std::time::Instant::now();
        
        // Reset timing counters
        self.computation_time = 0.0;
        self.file_io_time = 0.0;
        self.conversion_time = 0.0;
        
        // Initialize for this size
        self.current_size = current_size;
        self.current.clear();
        self.current_file_batch = start_batch;  // Start from specified batch
        self.current_file_list_count = 0;
        self.current_total_list_count = 0;
        self.new.clear();
        self.new_output_batch = 0;
        self.new_file_list_count = 0;
        
        // Count existing output files created before start_batch
        // This gives us the baseline count from previous runs
        let existing_count = count_existing_output_files_before_batch(
            &self.base_path,
            self.current_size + 1,
            start_batch
        );
        self.new_total_list_count = existing_count;
        
        // Process files starting from start_batch
        loop {
            debug_print(&format!("process_from_batch: loading batch {} for size {:02}", 
                self.current_file_batch, self.current_size));
            
            let loaded = self.refill_current_from_file();
            if loaded {
                debug_print(&format!("process_from_batch: loaded {} n-lists", 
                    self.current.len()));
                self.process_one_file_of_current_size_n(max);
                self.current_file_batch += 1;
            } else {
                debug_print(&format!("process_from_batch: no more files for size {:02}", 
                    self.current_size));
                break;
            }
        }
        
        let elapsed = start_time.elapsed();
        let elapsed_secs = elapsed.as_secs_f64();
        let overhead = elapsed_secs - self.computation_time - self.file_io_time - self.conversion_time;
        
        debug_print(&format!("process_from_batch: Finished processing size {:02} from batch {}", 
            self.current_size, start_batch));
        
        // Report total with breakdown
        created_a_total_of(self.new_total_list_count, self.current_size + 1, elapsed_secs);
        test_print(&format!("   ... timing breakdown: computation {:.2}s \
            ({:.1}%), file I/O {:.2}s ({:.1}%), conversion {:.2}s ({:.1}%), \
            overhead {:.2}s ({:.1}%)",
            self.computation_time, (self.computation_time / elapsed_secs * 100.0),
            self.file_io_time, (self.file_io_time / elapsed_secs * 100.0),
            self.conversion_time, (self.conversion_time / elapsed_secs * 100.0),
            overhead, (overhead / elapsed_secs * 100.0)));
        
        self.new_total_list_count
    }
}

impl Default for ListOfNSL {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to print large numbers with thousand separators and timing info
pub fn created_a_total_of(nb: u64, size: u8, elapsed_secs: f64) {
    let hours = (elapsed_secs / 3600.0) as u64;
    let minutes = ((elapsed_secs % 3600.0) / 60.0) as u64;
    let seconds = (elapsed_secs % 60.0) as u64;
    
    test_print(&format!("   ... created a total of {:>15} no-set-{:02} lists \
        in {:>10.2} seconds ({:02}h{:02}m{:02}s)", 
        nb.separated_string(), size, elapsed_secs, hours, minutes, seconds));
}

/// Filename helper - single source of truth for all file naming
/// 
/// Pattern: nsl_{source_size:02}_batch_{source_batch:03}_to_{target_size:02}_batch_{target_batch:03}.rkyv
/// 
/// Examples:
/// - Seed file: nsl_00_batch_000_to_03_batch_000.rkyv (source_size=0, target_size=3)
/// - Size 4 output: nsl_03_batch_000_to_04_batch_000.rkyv (from size 3 to create size 4)
/// - Size 5 input batch 0: Find files matching *_to_05_batch_000.rkyv
/// - Size 5 output from input batch 0: nsl_04_batch_000_to_05_batch_{000,001,002...}.rkyv

/// Generate output filename when saving
fn output_filename(
    base_path: &str,
    source_size: u8,
    source_batch: u16,
    target_size: u8,
    target_batch: u16
) -> String {
    use std::path::Path;
    let filename = format!(
        "nsl_{:02}_batch_{:03}_to_{:02}_batch_{:03}.rkyv",
        source_size, source_batch, target_size, target_batch
    );
    let path = Path::new(base_path).join(filename);
    path.to_string_lossy().to_string()
}

/// Find input filename for reading
/// Searches for file matching: *_to_{target_size}_batch_{target_batch}.rkyv
fn find_input_filename(
    base_path: &str,
    target_size: u8,
    target_batch: u16
) -> Option<String> {
    use std::fs;
    
    let pattern = format!("_to_{:02}_batch_{:03}.rkyv", target_size, target_batch);
    
    let entries = fs::read_dir(base_path).ok()?;
    
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with("nsl_") && name.ends_with(&pattern) {
                return Some(entry.path().to_string_lossy().to_string());
            }
        }
    }
    
    None
}

/// Count existing output files before a specific batch (for restart mode)
/// Counts files matching: nsl_*_to_{target_size}_batch_{batch}.rkyv where batch < restart_batch
fn count_existing_output_files_before_batch(
    base_path: &str,
    target_size: u8,
    restart_batch: u16
) -> u64 {
    use std::fs;
    let mut total = 0u64;
    
    let entries = match fs::read_dir(base_path) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    
    let pattern_prefix = format!("_to_{:02}_batch_", target_size);
    
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            // Check if this is an output file for our target size
            if name.starts_with("nsl_") && name.contains(&pattern_prefix) && name.ends_with(".rkyv") {
                // Extract the SOURCE batch number from filename
                // Format: nsl_XX_batch_YYY_to_ZZ_batch_BBB.rkyv
                // We need YYY (the source batch), not BBB (the target batch)
                if let Some(to_pos) = name.find("_to_") {
                    let before_to = &name[..to_pos];
                    if let Some(batch_pos) = before_to.rfind("_batch_") {
                        let batch_str = &before_to[batch_pos + 7..];
                        if let Ok(source_batch_num) = batch_str.parse::<u16>() {
                            // Only count files created from source batches < restart_batch
                            if source_batch_num < restart_batch {
                                let path = entry.path();
                                if let Some(vec_nlist) = read_from_file_serialized(path.to_str().unwrap()) {
                                    let count = vec_nlist.len() as u64;
                                    total += count;
                                    test_print(&format!("   ... counted {:>10} no-set-lists from {}",
                                        count.separated_string(), name));
                                    debug_print(&format!("count_existing: {} entries in {} (source batch {})",
                                        count, name, source_batch_num));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    debug_print(&format!("count_existing_output_files_before_batch: total {} entries for size {:02} before batch {:03}",
        total, target_size, restart_batch));
    total
}

/// Save NoSetListSerialized vector to file using rkyv
fn save_to_file_serialized(list: &Vec<NoSetListSerialized>, filename: &str) -> bool {
    debug_print(&format!("save_to_file_serialized: Serializing {} n-lists to {} using rkyv", 
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

/// Read NoSetListSerialized vector from file using rkyv with memory mapping
fn read_from_file_serialized(filename: &str) -> Option<Vec<NoSetListSerialized>> {
    debug_print(&format!("read_from_file_serialized: Loading n-lists from {} using rkyv", filename));
    
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
    
    debug_print(&format!("read_from_file_serialized: mapped {} bytes from {}", mmap.len(), filename));
    
    // Validate and deserialize
    match check_archived_root::<Vec<NoSetListSerialized>>(&mmap) {
        Ok(archived_vec) => {
            let deserialized: Vec<NoSetListSerialized> = archived_vec
                .deserialize(&mut rkyv::Infallible)
                .expect("Deserialization should not fail after validation");
            
            debug_print(&format!("read_from_file_serialized: deserialized {} n-lists", 
                deserialized.len()));
            Some(deserialized)
        },
        Err(e) => {
            debug_print(&format!("read_from_file_serialized: Validation error for {}: {:?}",
                filename, e));
            None
        }
    }
}
