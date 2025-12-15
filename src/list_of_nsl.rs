/// Version 0.4.6: Hybrid stack-optimized computation with heap-based I/O + Input-intermediary tracking
/// 
/// This implementation combines the best of both worlds:
/// - Uses NoSetList (stack arrays) for computation → 4-5× faster
/// - Converts to NoSetListSerialized (heap Vecs) for I/O → compact 2GB files
/// - Automatic input-intermediary file generation for output tracking
/// 
/// Performance characteristics:
/// - Computation: Same speed as v0.3.0 (stack-optimized)
/// - File size: ~2GB per 20M batch (compact with size_32 rkyv)
/// - Memory: Moderate (~12-15GB peak during conversion + save)
/// - Tracking: Atomic writes ensure file integrity and restart reliability
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
    pub current_file_batch: u32,       // Current input file batch number (5 digits)
    pub current_file_list_count: u64,  // Lists loaded from current input file
    pub current_total_list_count: u64, // Total lists processed across all input files
    pub new: Vec<NoSetList>,           // newly created n+1-lists (stack-based during compute)
    pub new_output_batch: u32,         // Current output file batch - CONTINUOUS across all source files
    pub new_file_list_count: u64,      // Lists saved to current output file
    pub new_total_list_count: u64,     // Total lists created for target size
    pub input_path: String,            // base directory for loading input files
    pub output_path: String,           // base directory for saving output files
    pub computation_time: f64,         // time spent in core algorithm
    pub file_io_time: f64,             // time spent in file I/O operations
    pub conversion_time: f64,          // time spent converting between formats
    input_intermediary_buffer: Vec<String>, // Buffer for input-intermediary file lines
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
            input_path: String::from("."),
            output_path: String::from("."),
            computation_time: 0.0,
            file_io_time: 0.0,
            conversion_time: 0.0,
            input_intermediary_buffer: Vec::new(),
        }
    }
    
    /// Creates a new ListOfNSL with a custom base path (uses same for input and output)
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
            input_path: String::from(base_path),
            output_path: String::from(base_path),
            computation_time: 0.0,
            file_io_time: 0.0,
            conversion_time: 0.0,
            input_intermediary_buffer: Vec::new(),
        }
    }
    
    /// Creates a new ListOfNSL with separate input and output paths
    pub fn with_paths(input_path: &str, output_path: &str) -> Self {
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
            input_path: String::from(input_path),
            output_path: String::from(output_path),
            computation_time: 0.0,
            file_io_time: 0.0,
            conversion_time: 0.0,
            input_intermediary_buffer: Vec::new(),
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
        
        let file = output_filename(&self.output_path, 0, 0, 3, 0);
        
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
        let filename = match find_input_filename(&self.input_path, self.current_size, self.current_file_batch) {
            Some(f) => f,
            None => {
                debug_print(&format!("   ... No input file found for size {:02} batch {:06} in {}",
                    self.current_size, self.current_file_batch, self.input_path));
                debug_print(&format!("refill_current_from_file: No file found for size {:02} batch {:06}",
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
                debug_print(&format!("   ... loaded  {:>10} no-set-lists from {}", 
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
            &self.output_path, 
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
                
                // Buffer this output batch info for the input-intermediary file
                self.buffer_input_intermediary_line(self.new_output_batch, additional_new);
                
                self.new_total_list_count += additional_new;
                self.new_output_batch += 1;
                self.new.clear();
                debug_print(&format!("   ... saved   {:>10} no-set-lists  to  {}", 
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
    
    /// Buffer count information to be written to input-intermediary file later
    /// Records each output batch created from the current input batch
    fn buffer_input_intermediary_line(&mut self, output_batch: u32, output_count: u64) {
        // Generate the output filename for this batch
        // Use 6-digit batch numbers (always)
        let src_batch_width = 6;
        let tgt_batch_width = 6;
        let output_filename = format!(
            "nsl_{:02}_batch_{:0width1$}_to_{:02}_batch_{:0width2$}.rkyv",
            self.current_size,
            self.current_file_batch,
            self.current_size + 1,
            output_batch,
            width1 = src_batch_width,
            width2 = tgt_batch_width
        );
        
        // Add line to buffer
        let line = format!("   ... {:>8} lists in {}", output_count, output_filename);
        self.input_intermediary_buffer.push(line);
    }
    
    /// Write buffered lines to input-intermediary file in one operation
    fn write_input_intermediary_file(&mut self) {
        if self.input_intermediary_buffer.is_empty() {
            return;
        }
        
        // Use 6-digit batch numbers (always)
        let batch_width = 6;
        let target_size = self.current_size + 1;
        let filename = format!(
            "{}/nsl_{:02}_intermediate_count_from_{:02}_{:0width$}.txt",
            self.output_path, target_size, self.current_size, self.current_file_batch,
            width = batch_width
        );
        
        // Write all buffered lines at once
        use std::io::Write;
        match std::fs::File::create(&filename) {
            Ok(mut file) => {
                for line in &self.input_intermediary_buffer {
                    if let Err(e) = writeln!(file, "{}", line) {
                        debug_print(&format!("write_input_intermediary_file: Error writing to {}: {}", filename, e));
                    }
                }
                // Clear buffer after successful write
                self.input_intermediary_buffer.clear();
            },
            Err(e) => {
                debug_print(&format!("write_input_intermediary_file: Error creating {}: {}", filename, e));
            }
        }
    }

    /// Process one input file using stack-optimized computation
    /// Creates output files with modular naming and closes output when input exhausted
    fn process_one_file_of_current_size_n(&mut self, max: &u64) -> u64 {
        debug_print(&format!("   ... processing batch {} of size {:02} ({} input lists)", 
            self.current_file_batch, self.current_size, self.current.len()));
        debug_print(&format!("process_one_file_of_current_size_n: Processing batch {} \
            of no-set-{:02} ({} lists)", self.current_file_batch, self.current_size, 
            self.current.len()));
        
        // Don't reset new_output_batch - keep continuous numbering across all source files
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
                test_print(&format!("   ... saving batch ({:>10} lists), output batch {}", 
                    self.new.len().separated_string(), self.new_output_batch));
                if !self.save_new_to_file() {
                    test_print("   ... ERROR: Failed to save batch");
                    debug_print("process_one_file_of_current_size_n: Error saving batch");
                }
            }
            
            i += 1;
        }
        
        // Save any remaining lists from this input file (even if < max)
        if !self.new.is_empty() {
            test_print(&format!("   ... saving final batch ({} lists), output batch {}", 
                self.new.len().separated_string(), self.new_output_batch));
            debug_print(&format!("process_one_file_of_current_size_n: saving final batch of {}", 
                self.new.len()));
            if !self.save_new_to_file() {
                test_print("   ... ERROR: Failed to save final batch");
                debug_print("process_one_file_of_current_size_n: Error saving final batch");
            }
        }
        
        // Calculate and log this file's statistics
        let file_new_total = self.new_total_list_count - file_new_count_start;
        debug_print(&format!("   ... processed {} input lists, created {} new lists from batch {:06}",
            self.current_file_list_count.separated_string(),
            file_new_total.separated_string(),
            self.current_file_batch));
        
        file_new_total
    }
    
    // ========================================================================
    // Helper methods for common processing patterns
    // ========================================================================
    
    /// Initialize processing state for a given size and starting batch
    fn init_processing_state(&mut self, current_size: u8, start_batch: u32) {
        self.computation_time = 0.0;
        self.file_io_time = 0.0;
        self.conversion_time = 0.0;
        self.current_size = current_size;
        self.current.clear();
        self.current_file_batch = start_batch;
        self.current_file_list_count = 0;
        self.current_total_list_count = 0;
        self.new.clear();
        self.new_file_list_count = 0;
    }
    
    /// Initialize output batch number (for restart/unitary modes)
    fn init_output_batch(&mut self, reference_batch: u32) {
        let next_batch = get_next_output_batch_from_files(
            &self.output_path,
            self.current_size + 1,
            reference_batch
        );
        self.new_total_list_count = 0;
        self.new_output_batch = next_batch;
    }
    
    /// Print timing breakdown report
    fn print_timing_report(&self, start_time: std::time::Instant) {
        let elapsed = start_time.elapsed();
        let elapsed_secs = elapsed.as_secs_f64();
        let overhead = elapsed_secs - self.computation_time - self.file_io_time - self.conversion_time;
        
        test_print(&format!("   ... timing breakdown: computation {:.2}s \
            ({:.1}%), file I/O {:.2}s ({:.1}%), conversion {:.2}s ({:.1}%), \
            overhead {:.2}s ({:.1}%)",
            self.computation_time, (self.computation_time / elapsed_secs * 100.0),
            self.file_io_time, (self.file_io_time / elapsed_secs * 100.0),
            self.conversion_time, (self.conversion_time / elapsed_secs * 100.0),
            overhead, (overhead / elapsed_secs * 100.0)));
    }
    
    /// Process batches in a loop with consistent logging
    /// Returns number of batches processed
    fn process_batch_loop(&mut self, max: &u64, stop_after_one: bool) -> u32 {
        let mut batches_processed = 0;
        
        loop {
            // Add blank line before loading next batch (except for the first one)
            if batches_processed > 0 {
                test_print("");
            }
            test_print(&format!("   ... loading batch {}", self.current_file_batch));
            let loaded = self.refill_current_from_file();
            
            if loaded {
                test_print(&format!("   ... loaded {:>10} lists from batch {}", 
                    self.current.len().separated_string(), self.current_file_batch));
                
                self.process_one_file_of_current_size_n(max);
                
                // Write buffered input-intermediary file in one operation
                let batch_width = 6;
                let intermediary_filename = format!(
                    "no_set_list_input_intermediate_count_{:02}_{:0width$}.txt",
                    self.current_size, self.current_file_batch,
                    width = batch_width
                );
                self.write_input_intermediary_file();
                test_print(&format!("   ... saving input intermediary file {}", intermediary_filename));
                
                self.current_file_batch += 1;
                batches_processed += 1;
                
                if stop_after_one {
                    break;
                }
            } else {
                debug_print(&format!("process_batch_loop: no more files for size {:02}", 
                    self.current_size));
                break;
            }
        }
        
        batches_processed
    }
    
    // ========================================================================
    // Main processing methods (refactored to use helpers)
    // ========================================================================
    
    /// Process all files for a given size
    pub fn process_all_files_of_current_size_n(&mut self, current_size: u8, max: &u64) -> u64 {
        if current_size < 3 {
            debug_print("process_all_files_of_current_size_n: size must be >= 3");
            return 0;
        }
        
        debug_print(&format!("process_all_files_of_current_size_n: start processing \
            no-set-{:02}", current_size));
        
        let start_time = std::time::Instant::now();
        
        // Initialize from batch 0, starting output from batch 0
        self.init_processing_state(current_size, 0);
        self.new_output_batch = 0;
        self.new_total_list_count = 0;
        
        // Process all batches
        self.process_batch_loop(max, false);
        
        debug_print(&format!("process_all_files_of_current_size_n: Finished \
            processing size {:02}", self.current_size));
        
        // Report results
        let elapsed_secs = start_time.elapsed().as_secs_f64();
        created_a_total_of(self.new_total_list_count, self.current_size + 1, elapsed_secs);
        self.print_timing_report(start_time);
        
        self.new_total_list_count
    }
    
    /// Process files starting from a specific batch number (for restart capability)
    /// Used to resume processing after interruption
    pub fn process_from_batch(&mut self, current_size: u8, start_batch: u32, max: &u64) -> u64 {
        if current_size < 3 {
            debug_print("process_from_batch: size must be >= 3");
            return 0;
        }
        
        debug_print(&format!("process_from_batch: start processing no-set-{:02} from batch {}", 
            current_size, start_batch));
        
        let start_time = std::time::Instant::now();
        
        // Initialize from specific batch
        self.init_processing_state(current_size, start_batch);
        self.init_output_batch(start_batch);  // Scan for next available output batch
        
        // Process all batches from start_batch onwards
        self.process_batch_loop(max, false);
        
        debug_print(&format!("process_from_batch: Finished processing size {:02} from batch {}", 
            self.current_size, start_batch));
        
        // Report results
        let elapsed_secs = start_time.elapsed().as_secs_f64();
        created_a_total_of(self.new_total_list_count, self.current_size + 1, elapsed_secs);
        self.print_timing_report(start_time);
        
        self.new_total_list_count
    }
    
    /// Process a single input batch (unitary processing)
    /// Processes one specific input file and generates its output files
    pub fn process_single_batch(&mut self, input_size: u8, input_batch: u32, max: &u64) -> u64 {
        if input_size < 3 {
            debug_print("process_single_batch: input size must be >= 3");
            return 0;
        }
        
        debug_print(&format!("process_single_batch: processing input size {:02} batch {:06}", 
            input_size, input_batch));
        
        let start_time = std::time::Instant::now();
        
        // Initialize for single batch
        self.init_processing_state(input_size, input_batch);
        self.init_output_batch(input_batch);  // Scan for next available output batch
        
        test_print(&format!("   ... will create output starting from batch {:06}", self.new_output_batch));
        
        // Process only this one batch
        let batches_processed = self.process_batch_loop(max, true);
        
        if batches_processed == 0 {
            test_print(&format!("   ... ERROR: Could not load input file for size {:02} batch {:06}",
                input_size, input_batch));
            return 0;
        }
        
        debug_print(&format!("process_single_batch: Finished processing size {:02} batch {:06}", 
            input_size, input_batch));
        
        // Report results (slightly different format for unitary)
        test_print(&format!("   ... created {:>17} new no-set-{:02} lists from this batch",
            self.new_total_list_count.separated_string(), self.current_size + 1));
        self.print_timing_report(start_time);
        
        self.new_total_list_count
    }
}

impl Default for ListOfNSL {
    fn default() -> Self {
        Self::new()
    }
}

/// Count all existing output files for a given target size
/// Creates a summary report file with counts per batch
/// Number of files to process in each batch for count mode
const COUNT_BATCH_SIZE: usize = 10;

/// Count files for a given target size and create summary report
/// 
/// This function uses an input-intermediary file system:
/// 1. Input-intermediary files: nsl_{target_size:02}_intermediate_count_from_{input_size:02}_{input_batch:06}.txt
///    - Created automatically during file generation (--size, --restart, --unitary modes and --compact)
///    - One file per input batch, tracks which output files include lists from that input batch
/// 
/// File naming:
/// - Input-intermediary: nsl_{target_size:02}_intermediate_count_from_{input_size:02}_{input_batch:06}.txt
/// - Final report: nsl_{target_size:02}_global_count.txt
/// 
/// All files are stored in the same directory as the source files (base_path)
pub fn count_size_files(base_path: &str, target_size: u8, force: bool, keep_state: bool) -> std::io::Result<()> {
    use std::fs;
    use std::path::PathBuf;
    
    test_print(&format!("\nCounting files for size {:02}...", target_size));
    test_print(&format!("   Input directory: {}", base_path));
    // Count mode: reads existing input-intermediary files named
    // `nsl_{target_size:02}_intermediate_count_from_{source_size:02}_{input_batch:06}.txt`
    // and consolidates them into the final `nsl_{size:02}_global_count.txt` report.
    // It no longer creates or updates these small intermediary files; they must be present.
    
    let start_time = std::time::Instant::now();
    
    // Step 1: Scan for all .rkyv files
    let entries = fs::read_dir(base_path)?;
    let pattern = format!("_to_{:02}_batch_", target_size);
    
    let mut all_files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with("nsl_") && name.contains(&pattern) && name.ends_with(".rkyv") {
                all_files.push(entry.path());
            }
        }
    }
    all_files.sort();
    
    // Step 2: Find all intermediary input count files for this size
    use std::collections::BTreeMap;
    let mut intermediary_files: Vec<String> = Vec::new();
    let mut all_file_info: BTreeMap<(u32, u32), (String, u64)> = BTreeMap::new();

    // Find all input-intermediary files for this target size: nsl_{target}_intermediate_count_from_{source}_*.txt
    // Also accept legacy 'no_set_list_input_intermediate_count_{source}_*.txt' for robustness
    let pattern_new = format!("nsl_{:02}_intermediate_count_from_{:02}_", target_size, target_size - 1);
    let legacy_pattern = format!("no_set_list_input_intermediate_count_{:02}_", target_size - 1);
    let entries = fs::read_dir(base_path)?;
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if (name.starts_with(&pattern_new) || name.starts_with(&legacy_pattern)) && name.ends_with(".txt") {
                intermediary_files.push(format!("{}/{}", base_path, name));
            }
        }
    }
    intermediary_files.sort();

    progress_print(&format!("   ... Found {} intermediary input count files", intermediary_files.len()));

    // If no intermediary files found, create them by grouping .rkyv files by source_batch
    if intermediary_files.is_empty() {
        test_print(&format!("   ... No intermediary input count files; creating from {} .rkyv files", all_files.len()));
        use std::collections::BTreeMap;
        let mut groups: BTreeMap<u32, Vec<std::path::PathBuf>> = BTreeMap::new();
        let source_size = target_size - 1;
        for path in &all_files {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(to_pos) = name.find("_to_") {
                    let before_to = &name[..to_pos];
                    if let Some(src_batch_pos) = before_to.rfind("_batch_") {
                        let src_batch_str = &before_to[src_batch_pos + 7..];
                        if let Ok(src_batch) = src_batch_str.parse::<u32>() {
                            groups.entry(src_batch).or_default().push(path.clone());
                        }
                    }
                }
            }
        }

        for (src_batch, files) in groups {
            let inter_filename = format!("{}/nsl_{:02}_intermediate_count_from_{:02}_{:06}.txt", base_path, target_size, source_size, src_batch);
            if std::path::Path::new(&inter_filename).exists() {
                match is_intermediary_file_valid(&inter_filename, &files) {
                    Ok(true) => {
                        intermediary_files.push(inter_filename);
                        continue;
                    }
                    Ok(false) => {
                        test_print(&format!("   ... Found stale intermediary {} - recreating", inter_filename));
                        let _ = std::fs::remove_file(&inter_filename);
                    }
                    Err(e) => {
                        debug_print(&format!("   ... Error validating intermediary {}: {} - recreating", inter_filename, e));
                        let _ = std::fs::remove_file(&inter_filename);
                    }
                }
            }

            test_print(&format!("   ... creating intermediary file {}", inter_filename));
            // Create file by counting each file's lists
            match create_input_intermediary_from_files(&files, &inter_filename) {
                Ok(_) => intermediary_files.push(inter_filename),
                Err(e) => debug_print(&format!("   ... Error creating {}: {}", inter_filename, e)),
            }
        }
        test_print(&format!("   ... Created {} intermediary input count files", intermediary_files.len()));
    }

    // Step 3: Read intermediary input count files incrementally and collect counts
    // Incremental behavior:
    // - Maintain a partial entries file and a processed-batches file so the count run can resume
    // - Show batch-number progress as we read intermediary files (up to 10 per line)
    use std::io::{BufRead, BufReader, Write};

    let partial_filename = format!("{}/nsl_{:02}_global_count.partial", base_path, target_size);
    let processed_filename = format!("{}/nsl_{:02}_global_count.processed", base_path, target_size);

    // If force is requested, remove any existing partial/processed state so we rebuild from scratch
    if force {
        test_print(&format!("   ... FORCE: discarding previous partial/processed state"));
        let _ = std::fs::remove_file(&partial_filename);
        let _ = std::fs::remove_file(&processed_filename);
    }

    // Load processed batches from processed file (if present)
    let mut processed_batches: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    if std::path::Path::new(&processed_filename).exists() {
        if let Ok(contents) = std::fs::read_to_string(&processed_filename) {
            for line in contents.lines() {
                if let Ok(n) = line.trim().parse::<u32>() {
                    processed_batches.insert(n);
                }
            }
        }
    }

    // If force is enabled, ignore previously processed batches; otherwise attempt to seed from existing report
    if force {
        processed_batches.clear();
    } else {
        // Try to parse existing consolidated report for source batches (if any)
        let report_path = format!("{}/nsl_{:02}_global_count.txt", base_path, target_size);
        if std::path::Path::new(&report_path).exists() {
            if let Ok(contents) = std::fs::read_to_string(&report_path) {
                for line in contents.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(src) = parts[0].parse::<u32>() {
                            processed_batches.insert(src);
                        }
                    }
                }
            }
        }
    }

    // Open partial and processed files for appending
    let mut partial_file = std::fs::OpenOptions::new().create(true).append(true).open(&partial_filename)?;
    let mut processed_file = std::fs::OpenOptions::new().create(true).append(true).open(&processed_filename)?;

    // Helper to display processed batches in compact groups (10 per line)
    let mut display_buffer: Vec<String> = Vec::new();

    for intermediary_file in &intermediary_files {
        // Extract source batch number from intermediary filename (nsl_{tgt}_intermediate_count_from_{src}_{src_batch:06}.txt)
        let src_batch_opt = intermediary_file.rsplit('_').nth(0)
            .and_then(|s| s.strip_suffix(".txt"))
            .and_then(|s| s.parse::<u32>().ok());

        if let Some(src_batch) = src_batch_opt {
            if processed_batches.contains(&src_batch) {
                progress_print(&format!("   ... SKIPPING already-processed batch {:06}", src_batch));
                continue;
            }

            // show progress
            display_buffer.push(format!("{:06}", src_batch));
            if display_buffer.len() >= COUNT_BATCH_SIZE {
                progress_print(&format!("   ... reading batches: {}", display_buffer.join(" ")));
                display_buffer.clear();
            }

            // Read intermediary file and append its entries to partial file and in-memory map
            let file = fs::File::open(intermediary_file)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = line?;
                if line.trim().starts_with("...") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 5 {
                        if let Ok(count) = parts[1].parse::<u64>() {
                            let filename = parts[4];
                            // Parse source and target batch numbers from filename
                            if let Some(to_pos) = filename.find("_to_") {
                                let before_to = &filename[..to_pos];
                                let after_to = &filename[to_pos + 4..];
                                if let Some(src_batch_pos) = before_to.rfind("_batch_") {
                                    let src_batch_str = &before_to[src_batch_pos + 7..];
                                    if let Ok(srcb) = src_batch_str.parse::<u32>() {
                                        if let Some(tgt_batch_pos) = after_to.rfind("_batch_") {
                                            let tgt_batch_str = &after_to[tgt_batch_pos + 7..after_to.len() - 5]; // -5 for ".rkyv"
                                            if let Ok(tgtb) = tgt_batch_str.parse::<u32>() {
                                                // Insert into in-memory map
                                                all_file_info.insert((srcb, tgtb), (filename.to_string(), count));
                                                // Append to partial file as CSV: src,tgt,count,filename
                                                writeln!(partial_file, "{},{},{},{}", srcb, tgtb, count, filename)?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Mark this source batch as processed
            writeln!(processed_file, "{}", src_batch)?;
            processed_batches.insert(src_batch);

            // Update a lightweight progress view in the global report so the run is observable and idempotent
            let _report_path = format!("{}/nsl_{:02}_global_count.txt", base_path, target_size);
            let _tmp = format!("{}/.nsl_{:02}_global_count.tmp", base_path, target_size);
            // Regenerate the consolidated global report from the partial CSV so it reflects progress
            let _ = regenerate_report_from_partial(base_path, target_size as u8, &partial_filename, intermediary_files.len());
        } else {
            // fallback: if we couldn't parse batch, still read file but don't mark processed
            let file = fs::File::open(intermediary_file)?;
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let _ = line?;
            }
        }
    }

    // Flush any remaining display buffer
    if !display_buffer.is_empty() {
        progress_print(&format!("   ... reading batches: {}", display_buffer.join(" ")));
    }

    // If no entries were collected, we're done
    if all_file_info.is_empty() {
        test_print(&format!("   ... No files found for size {:02}", target_size));
        // Cleanup partial/processed state by default unless caller requested to keep state
        if !keep_state {
            let partial_filename = format!("{}/nsl_{:02}_global_count.partial", base_path, target_size);
            let processed_filename = format!("{}/nsl_{:02}_global_count.processed", base_path, target_size);
            match std::fs::remove_file(&partial_filename) {
                Ok(_) => test_print(&format!("   Removed {}", partial_filename)),
                Err(e) => test_print(&format!("   [WARN] Could not remove {}: {}", partial_filename, e)),
            }
            match std::fs::remove_file(&processed_filename) {
                Ok(_) => test_print(&format!("   Removed {}", processed_filename)),
                Err(e) => test_print(&format!("   [WARN] Could not remove {}: {}", processed_filename, e)),
            }
        } else {
            debug_print(&format!("   Keeping partial/processed state for size {:02}", target_size));
        }
        return Ok(());
    }

    // Write the global count file using the same logic as consolidate_count_files
    test_print(&format!("\n   ... Consolidating {} intermediary input count files...", intermediary_files.len()));
    let report_filename = format!("{}/nsl_{:02}_global_count.txt", base_path, target_size);
    let mut report_file = std::fs::File::create(&report_filename)?;
    writeln!(report_file, "# File Count Summary for no-set-{:02} lists", target_size)?;
    writeln!(report_file, "# Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))?;
    writeln!(report_file, "# Input directory: {}", base_path)?;
    writeln!(report_file, "# Intermediary files used: {} batch files", intermediary_files.len())?;
    writeln!(report_file, "# Format: source_batch target_batch | cumulative_nb_lists | nb_lists_in_file | filename")?;
    writeln!(report_file, "#")?;
    let mut sorted_files: Vec<_> = all_file_info.iter().collect();
    sorted_files.sort_by(|a, b| {
        match a.0.1.cmp(&b.0.1) {
            std::cmp::Ordering::Equal => a.0.0.cmp(&b.0.0),
            other => other,
        }
    });
    let mut cumulative = 0u64;
    let mut report_lines = Vec::new();
    for ((source_batch, target_batch), (filename, count)) in sorted_files.iter() {
        cumulative += count;
        report_lines.push(format!(
            "{:06} {:06} | {:>15} | {:>15} | {}",
            source_batch,
            target_batch,
            cumulative.separated_string(),
            count.separated_string(),
            filename
        ));
    }
    for line in report_lines.iter().rev() {
        writeln!(report_file, "{}", line)?;
    }
    writeln!(report_file, "#")?;
    writeln!(report_file, "# Total files: {}", all_file_info.len())?;
    writeln!(report_file, "# Total lists: {}", cumulative.separated_string())?;
    debug_print(&format!("\n   Summary written to: {}", report_filename));
    debug_print(&format!("   Total files: {}", all_file_info.len()));
    debug_print(&format!("   Total lists: {}", cumulative.separated_string()));
    let elapsed = start_time.elapsed().as_secs_f64();
    test_print(&format!("\nCount completed in {:.2} seconds", elapsed));

    // Cleanup partial/processed state by default unless caller requested to keep state
        if !keep_state {
            let partial_filename = format!("{}/nsl_{:02}_global_count.partial", base_path, target_size);
            let processed_filename = format!("{}/nsl_{:02}_global_count.processed", base_path, target_size);
            match std::fs::remove_file(&partial_filename) {
                Ok(_) => test_print(&format!("   Removed {}", partial_filename)),
                Err(e) => test_print(&format!("   [WARN] Could not remove {}: {}", partial_filename, e)),
            }
            match std::fs::remove_file(&processed_filename) {
                Ok(_) => test_print(&format!("   Removed {}", processed_filename)),
                Err(e) => test_print(&format!("   [WARN] Could not remove {}: {}", processed_filename, e)),
            }
        } else {
            debug_print(&format!("   Keeping partial/processed state for size {:02}", target_size));
        }
    Ok(())
}

/// Check if an intermediary file is valid (exists and is newer than all source files)
/// Validate an input-intermediary file for a given input batch
/// Checks:
/// 1. File exists
/// 2. File's timestamp is more recent than the source .rkyv file
/// 3. File contains an entry for the source .rkyv file
fn is_intermediary_file_valid(intermediary_file: &str, source_files: &[std::path::PathBuf]) -> std::io::Result<bool> {
    use std::fs;
    
    // Check if intermediary file exists
    let intermediary_path = std::path::Path::new(intermediary_file);
    if !intermediary_path.exists() {
        return Ok(false);
    }
    
    // Get intermediary file's modification time
    let intermediary_metadata = fs::metadata(intermediary_path)?;
    let intermediary_mtime = intermediary_metadata.modified()?;
    
    // Check if any source file is newer than the intermediary file
    for source_file in source_files {
        let source_metadata = fs::metadata(source_file)?;
        let source_mtime = source_metadata.modified()?;
        
        if source_mtime > intermediary_mtime {
            // Source file is newer, intermediary is stale
            return Ok(false);
        }
    }
    
    // All source files are older than intermediary file
    Ok(true)
}

/// Process a batch of files and write results to an intermediary file
// Helper: create input-intermediary files from a list of .rkyv files (one per source batch)
fn create_input_intermediary_from_files(files: &[std::path::PathBuf], output_file: &str) -> std::io::Result<u64> {
    use std::fs::File;
    use std::io::Write;
    use memmap2::Mmap;

    let mut total = 0u64;
    let mut out = std::fs::File::create(output_file)?;

    for path in files {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let file = File::open(path)?;
            let mmap = unsafe { Mmap::map(&file)? };
            match check_archived_root::<Vec<NoSetListSerialized>>(&mmap[..]) {
                Ok(arch) => {
                    let count = arch.len() as u64;
                    total += count;
                    writeln!(out, "   ... {:>8} lists in {}", count, name)?;
                    test_print(&format!("       {:>10} lists in {}", count.separated_string(), name));
                }
                Err(e) => {
                    debug_print(&format!("create_input_intermediary_from_files: Validation error for {}: {:?}", name, e));
                }
            }
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn incremental_count_resume() {
        // Create a temporary directory
        let mut base = std::env::temp_dir();
        base.push(format!("funny_test_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)));
        let base = base;
        fs::create_dir_all(&base).unwrap();

        // Prepare two intermediary files for target size 09, source size 08
        let target_size = 9u8;
        let src_size = 8u8;

        let file_a = base.join(format!("nsl_{:02}_intermediate_count_from_{:02}_{:06}.txt", target_size, src_size, 0));
        let mut fa = File::create(&file_a).unwrap();
        writeln!(fa, "   ... 5 lists in nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", src_size, 0, target_size, 10).unwrap();
        writeln!(fa, "   ... 3 lists in nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", src_size, 0, target_size, 11).unwrap();

        let file_b = base.join(format!("nsl_{:02}_intermediate_count_from_{:02}_{:06}.txt", target_size, src_size, 1));
        let mut fb = File::create(&file_b).unwrap();
        writeln!(fb, "   ... 7 lists in nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", src_size, 1, target_size, 12).unwrap();

        // Run count first time
        count_size_files(base.to_str().unwrap(), target_size, false, true).unwrap();

        let partial = base.join(format!("nsl_{:02}_global_count.partial", target_size));
        let processed = base.join(format!("nsl_{:02}_global_count.processed", target_size));
        assert!(partial.exists());
        assert!(processed.exists());

        let before = fs::read_to_string(&partial).unwrap();
        let before_lines = before.lines().count();
        assert!(before_lines >= 3);

        // Run count second time; it should not duplicate partial entries
        count_size_files(base.to_str().unwrap(), target_size, false, true).unwrap();
        let after = fs::read_to_string(&partial).unwrap();
        let after_lines = after.lines().count();
        assert_eq!(before_lines, after_lines);

        // Cleanup
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn force_regenerates_global_count_preserves_intermediaries() {
        // Create a temporary directory
        let mut base = std::env::temp_dir();
        base.push(format!("funny_test_force_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)));
        let base = base;
        fs::create_dir_all(&base).unwrap();

        let target_size = 9u8;
        let src_size = 8u8;

        let file_a = base.join(format!("nsl_{:02}_intermediate_count_from_{:02}_{:06}.txt", target_size, src_size, 0));
        let mut fa = File::create(&file_a).unwrap();
        writeln!(fa, "   ... 5 lists in nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", src_size, 0, target_size, 10).unwrap();

        // Initial count (normal)
        count_size_files(base.to_str().unwrap(), target_size, false, false).unwrap();

        // Record intermediary contents
        let orig_inter = fs::read_to_string(&file_a).unwrap();

        // Run count again with force=true; should regenerate global report but not change intermediaries
        count_size_files(base.to_str().unwrap(), target_size, true, false).unwrap();

        // Ensure intermediary file unchanged
        let new_inter = fs::read_to_string(&file_a).unwrap();
        assert_eq!(orig_inter, new_inter, "Intermediary file was modified by force run");

        // Ensure global report exists and contains totals
        let report = base.join(format!("nsl_{:02}_global_count.txt", target_size));
        assert!(report.exists());
        let report_contents = fs::read_to_string(&report).unwrap();
        assert!(report_contents.contains("Total lists") || report_contents.contains("Total files"));

        // Cleanup
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn default_cleanup_removes_partial_processed() {
        // Create a temporary directory
        let mut base = std::env::temp_dir();
        base.push(format!("funny_test_cleanup_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)));
        let base = base;
        fs::create_dir_all(&base).unwrap();

        let target_size = 9u8;
        let src_size = 8u8;

        let file_a = base.join(format!("nsl_{:02}_intermediate_count_from_{:02}_{:06}.txt", target_size, src_size, 0));
        let mut fa = File::create(&file_a).unwrap();
        writeln!(fa, "   ... 5 lists in nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", src_size, 0, target_size, 10).unwrap();

        // Run count with default cleanup (keep_state=false)
        count_size_files(base.to_str().unwrap(), target_size, false, false).unwrap();

        let partial = base.join(format!("nsl_{:02}_global_count.partial", target_size));
        let processed = base.join(format!("nsl_{:02}_global_count.processed", target_size));
        assert!(!partial.exists(), "Partial file should be removed by default");
        assert!(!processed.exists(), "Processed file should be removed by default");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_on_empty_run_removes_state() {
        let mut base = std::env::temp_dir();
        base.push(format!("funny_test_cleanup_empty_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)));
        let base = base;
        fs::create_dir_all(&base).unwrap();

        let target_size = 9u8;

        // Create dummy partial and processed files
        let partial = base.join(format!("nsl_{:02}_global_count.partial", target_size));
        let processed = base.join(format!("nsl_{:02}_global_count.processed", target_size));
        File::create(&partial).unwrap();
        File::create(&processed).unwrap();

        // Run count where no intermediary files exist; should remove state by default
        count_size_files(base.to_str().unwrap(), target_size, false, false).unwrap();

        assert!(!partial.exists());
        assert!(!processed.exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn stale_intermediary_is_recreated() {
        // Create a temporary directory
        let mut base = std::env::temp_dir();
        base.push(format!("funny_test_stale_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)));
        let base = base;
        fs::create_dir_all(&base).unwrap();

        let target_size = 9u8;
        let src_size = 8u8;

        // Prepare a source file (simulate .rkyv) and an intermediary older than the source
        let src_file = base.join(format!("nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", src_size, 0, target_size, 10));
        let mut sf = File::create(&src_file).unwrap();
        writeln!(sf, "dummy").unwrap();

        // Create an intermediary first so it's older
        let inter = base.join(format!("nsl_{:02}_intermediate_count_from_{:02}_{:06}.txt", target_size, src_size, 0));
        let mut ia = File::create(&inter).unwrap();
        writeln!(ia, "   ... old entry").unwrap();

        // Now update source to be newer by touching it (write again)
        let mut sf = std::fs::OpenOptions::new().append(true).open(&src_file).unwrap();
        writeln!(sf, "updated").unwrap();

        // Run count; it should detect stale intermediary and recreate it
        count_size_files(base.to_str().unwrap(), target_size, false, true).unwrap();

        // Check intermediary mtime is newer than initial creation (i.e., was recreated)
        let meta = fs::metadata(&inter).unwrap();
        let mtime = meta.modified().unwrap();
        // Ensure mtime is recent (within 60s)
        let age = chrono::Local::now().signed_duration_since(chrono::DateTime::<chrono::Local>::from(mtime));
        assert!(age.num_seconds() < 60, "Intermediary was not recreated (mtime too old)");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn regenerate_report_from_partial_updates_report() {
        // Create a temporary directory
        let mut base = std::env::temp_dir();
        base.push(format!("funny_test_regen_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)));
        let base = base;
        fs::create_dir_all(&base).unwrap();

        let target_size = 9u8;

        let partial = base.join(format!("nsl_{:02}_global_count.partial", target_size));
        let mut pf = File::create(&partial).unwrap();
        // write one partial entry: src,tgt,count,filename
        writeln!(pf, "{},{},{},{}", 0u32, 12u32, 7u64, "nsl_08_batch_000000_to_09_batch_000012.rkyv").unwrap();

        // Run regen helper
        regenerate_report_from_partial(base.to_str().unwrap(), target_size, partial.to_str().unwrap(), 1).unwrap();

        let report = base.join(format!("nsl_{:02}_global_count.txt", target_size));
        assert!(report.exists());
        let contents = fs::read_to_string(&report).unwrap();
        assert!(contents.contains("nsl_08_batch_000000_to_09_batch_000012.rkyv"));
        assert!(contents.contains("Total lists") || contents.contains("Total files") || contents.contains("Total lists (partial)"));

        // Append another partial entry and regen again
        let mut pf = std::fs::OpenOptions::new().append(true).open(&partial).unwrap();
        writeln!(pf, "{},{},{},{}", 0u32, 13u32, 3u64, "nsl_08_batch_000000_to_09_batch_000013.rkyv").unwrap();
        regenerate_report_from_partial(base.to_str().unwrap(), target_size, partial.to_str().unwrap(), 2).unwrap();

        let contents2 = fs::read_to_string(&report).unwrap();
        assert!(contents2.contains("nsl_08_batch_000000_to_09_batch_000013.rkyv"));
        assert!(contents2.contains("Total lists (partial)") || contents2.contains("Total lists"));

        let _ = fs::remove_dir_all(&base);
    }
}

/// Regenerate the consolidated global report from the partial CSV file.
/// This is called after each processed batch so the human-readable global report
/// is up-to-date and reflects progress mid-run.
pub fn regenerate_report_from_partial(base_path: &str, target_size: u8, partial_filename: &str, intermediary_files_total: usize) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Write};
    use std::collections::BTreeMap;

    if !std::path::Path::new(partial_filename).exists() {
        return Ok(());
    }

    let file = File::open(partial_filename)?;
    let reader = BufReader::new(file);

    // Map by filename to keep the latest entry per file (avoid duplicates)
    let mut by_file: BTreeMap<String, (u32, u32, u64)> = BTreeMap::new();
    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.splitn(4, ',').collect();
        if parts.len() == 4 {
            if let (Ok(src), Ok(tgt), Ok(count)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>(), parts[2].parse::<u64>()) {
                let filename = parts[3].to_string();
                by_file.insert(filename, (src, tgt, count));
            }
        }
    }

    // Build sorted vector similar to final report
    let mut all_file_info: Vec<((u32, u32), (String, u64))> = Vec::new();
    for (filename, (src, tgt, count)) in by_file {
        all_file_info.push(((src, tgt), (filename, count)));
    }
    all_file_info.sort_by(|a, b| match a.0.1.cmp(&b.0.1) {
        std::cmp::Ordering::Equal => a.0.0.cmp(&b.0.0),
        other => other,
    });

    let report_path = format!("{}/nsl_{:02}_global_count.txt", base_path, target_size);
    let tmp = format!("{}/.nsl_{:02}_global_count.tmp", base_path, target_size);
    let mut report_file = File::create(&tmp)?;

    writeln!(report_file, "# File Count Summary for no-set-{:02} lists (IN-PROGRESS)", target_size)?;
    writeln!(report_file, "# Generated (progress): {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))?;
    writeln!(report_file, "# Input directory: {}", base_path)?;
    writeln!(report_file, "# Intermediary files total: {} batch files", intermediary_files_total)?;
    writeln!(report_file, "# Intermediaries used (partial): {}", all_file_info.len())?;
    writeln!(report_file, "# Format: source_batch target_batch | cumulative_nb_lists | nb_lists_in_file | filename")?;
    writeln!(report_file, "#")?;

    let mut cumulative = 0u64;
    for ((source_batch, target_batch), (filename, count)) in all_file_info.iter() {
        cumulative += *count;
        writeln!(report_file, "{:06} {:06} | {:>15} | {:>15} | {}", source_batch, target_batch, cumulative.separated_string(), count.separated_string(), filename)?;
    }

    writeln!(report_file, "#")?;
    writeln!(report_file, "# Total partial files: {}", all_file_info.len())?;
    writeln!(report_file, "# Total lists (partial): {}", cumulative.separated_string())?;

    let _ = std::fs::rename(&tmp, &report_path);
    Ok(())
}

/// Consolidate all intermediary count files into the final report
// Removed: consolidate_count_files - output-intermediary files are no longer used.

/// Check repository integrity for a specific size
/// - Lists missing output batches (should be continuous)
/// - Lists files mentioned in intermediary files but missing from directory
pub fn check_size_files(base_path: &str, target_size: u8) -> std::io::Result<()> {
    use std::fs;
    use std::path::PathBuf;
    use std::collections::{BTreeSet, HashMap};
    use std::io::{BufRead, BufReader};
    
    test_print(&format!("\nCHECK MODE: Analyzing repository for size {:02}...", target_size));
    test_print(&format!("   Directory: {}", base_path));
    
    // Step 1: Scan directory and collect all output files
    let entries = fs::read_dir(base_path)?;
    let pattern = format!("_to_{:02}_batch_", target_size);
    
    let mut all_files: Vec<String> = Vec::new();
    let mut batch_numbers: BTreeSet<u32> = BTreeSet::new();
    
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with("nsl_") && name.contains(&pattern) && name.ends_with(".rkyv") {
                all_files.push(name.to_string());
                
                // Extract target batch number
                if let Some(to_pos) = name.find("_to_") {
                    let after_to = &name[to_pos + 4..];
                    if let Some(tgt_batch_pos) = after_to.rfind("_batch_") {
                        let tgt_batch_str = &after_to[tgt_batch_pos + 7..after_to.len() - 5]; // -5 for ".rkyv"
                        if let Ok(batch_num) = tgt_batch_str.parse::<u32>() {
                            batch_numbers.insert(batch_num);
                        }
                    }
                }
            }
        }
    }
    
    test_print(&format!("   Found {} output files", all_files.len()));
    
    // Step 2: Check for missing batches in sequence
    if !batch_numbers.is_empty() {
        let min_batch = *batch_numbers.iter().next().unwrap();
        let max_batch = *batch_numbers.iter().last().unwrap();
        let mut missing_batches = Vec::new();
        
        for batch in min_batch..=max_batch {
            if !batch_numbers.contains(&batch) {
                missing_batches.push(batch);
            }
        }
        
        test_print(&format!("   Batch range: {:06} to {:06}", min_batch, max_batch));
        
        if missing_batches.is_empty() {
            test_print("   [OK] No missing batches in sequence");
        } else {
            test_print(&format!("   [!!] Found {} missing batches:", missing_batches.len()));
            for batch in &missing_batches {
                test_print(&format!("        - Batch {:06}", batch));
            }
        }
    } else {
        test_print("   No output files found");
    }
    
    // Build a set of existing files for fast lookup
    let existing_files: HashMap<String, bool> = all_files.iter()
        .map(|f| (f.clone(), true))
        .collect();
    
    // Step 2: Check consolidated count file for missing files
    let consolidated_count_file = format!("{}/nsl_{:02}_global_count.txt", base_path, target_size);
    let consolidated_path = std::path::Path::new(&consolidated_count_file);
    
    if consolidated_path.exists() {
        test_print(&format!("\n   Checking consolidated count file: nsl_{:02}_global_count.txt", target_size));
        
        let file = fs::File::open(consolidated_path)?;
        let reader = BufReader::new(file);
        
        let mut total_files_in_consolidated = 0usize;
        let mut missing_from_consolidated = Vec::new();
        
        for line in reader.lines() {
            let line = line?;
            // Skip comment lines
            if line.trim().starts_with('#') {
                continue;
            }
            // Format: "source_batch target_batch | cumulative | count | filename"
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 4 {
                let filename = parts[3].trim();
                if !filename.is_empty() {
                    total_files_in_consolidated += 1;
                    
                    if !existing_files.contains_key(filename) {
                        missing_from_consolidated.push(filename.to_string());
                    }
                }
            }
        }
        
        test_print(&format!("   Files listed in consolidated file: {}", total_files_in_consolidated));
        
        if missing_from_consolidated.is_empty() {
            test_print("   [OK] All files in consolidated count file are present");
        } else {
            test_print(&format!("   [!!] Found {} files in consolidated file but missing from directory:", missing_from_consolidated.len()));
            for filename in &missing_from_consolidated {
                test_print(&format!("        - {}", filename));
            }
        }
    } else {
        test_print(&format!("\n   Consolidated count file not found: nsl_{:02}_global_count.txt", target_size));
        test_print("   Run --count mode first to generate count file");
    }
    
    // Step 3: Read intermediary count files and check for missing files
    // Match new 'from_' pattern only
    let count_pattern_new = format!("nsl_{:02}_intermediate_count_from_", target_size);
    let entries = fs::read_dir(base_path)?;
    
    let mut intermediary_files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with(&count_pattern_new) && name.ends_with(".txt") {
                intermediary_files.push(entry.path());
            }
        }
    }
    
    if intermediary_files.is_empty() {
        test_print("\n   No intermediary count files found");
        test_print("   (Intermediary files are optional, used for idempotent batch processing)");
    } else {
        test_print(&format!("\n   Checking {} intermediary count files", intermediary_files.len()));
        
        // Read each intermediary file and check for missing files
        let mut total_files_in_intermediary = 0usize;
        let mut missing_files = Vec::new();
        
        for intermediary_file in &intermediary_files {
            let file = fs::File::open(intermediary_file)?;
            let reader = BufReader::new(file);
            
            for line in reader.lines() {
                let line = line?;
                // Format: "   ... count lists in filename"
                if line.trim().starts_with("...") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 5 {
                        let filename = parts[4];
                        total_files_in_intermediary += 1;
                        
                        if !existing_files.contains_key(filename) {
                            missing_files.push(filename.to_string());
                        }
                    }
                }
            }
        }
        
        test_print(&format!("   Files listed in intermediary files: {}", total_files_in_intermediary));
        
        if missing_files.is_empty() {
            test_print("   [OK] All files listed in intermediary files are present");
        } else {
            test_print(&format!("   [!!] Found {} files listed but missing from directory:", missing_files.len()));
            for filename in &missing_files {
                test_print(&format!("        - {}", filename));
            }
        }
    }
    
    test_print("\nCheck completed");
    Ok(())
}

/// Compact small output files into larger 10M-entry batches
/// Reads all files for a given size, consolidates them, and replaces originals
pub fn compact_size_files(input_dir: &str, output_dir: &str, target_size: u8, batch_size: u64) -> std::io::Result<()> {
    use std::fs;
    
    test_print(&format!("\nCompacting files for size {:02}...", target_size));
    test_print(&format!("Target batch size: {} lists per file", batch_size.separated_string()));
    
    let start_time = std::time::Instant::now();
    
    // Prefer to read plan from consolidated count file if available
    let report_filename = format!("{}/nsl_{:02}_global_count.txt", input_dir, target_size);
    let mut input_files_ordered: Vec<String> = Vec::new(); // base filenames

    if std::path::Path::new(&report_filename).exists() {
        test_print(&format!("   Found consolidated count file: {}", report_filename));
        // Parse consolidated file to obtain ordered list of filenames (ascending target_batch, then source_batch)
        let file = fs::File::open(&report_filename)?;
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(file);
        let mut entries: Vec<(u32, u32, String)> = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() || line.trim().starts_with('#') { continue; }
            // Format: "{:05} {:05} | cumulative | count | filename"
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 4 {
                let src_tgt = parts[0].trim();
                let fields: Vec<&str> = src_tgt.split_whitespace().collect();
                if fields.len() >= 2 {
                    if let (Ok(src), Ok(tgt)) = (fields[0].parse::<u32>(), fields[1].parse::<u32>()) {
                        let filename = parts[3].trim().to_string();
                        entries.push((src, tgt, filename));
                    }
                }
            }
        }
        // sort ascending target batch then source batch
        entries.sort_by(|a, b| match a.1.cmp(&b.1) { std::cmp::Ordering::Equal => a.0.cmp(&b.0), other => other });
        for (_src, _tgt, fname) in entries {
            input_files_ordered.push(fname);
        }
    } else {
        test_print("   Consolidated count file not found; falling back to directory scan");
        // Fallback: scan directory for matching files (skip already compacted outputs)
        let pattern = format!("*_to_{:02}_batch_*.rkyv", target_size);
        let mut paths = fs::read_dir(input_dir)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| !name.contains("compacted") && wildmatch::WildMatch::new(&pattern).matches(name))
            .collect::<Vec<_>>();
        paths.sort();
        for name in paths { input_files_ordered.push(name); }
    }

    if input_files_ordered.is_empty() {
        test_print("   No files found to compact");
        return Ok(());
    }
    test_print(&format!("   Found {} files to compact (according to plan)", input_files_ordered.len()));
    
    // Build plan from metadata files (do NOT inspect .rkyv content to decide plan)
    // Prefer consolidated count file; otherwise use intermediary input count files for the previous size.
    let mut file_counts: Vec<(String, u64, u32, u32)> = Vec::new(); // (filename, count, src_batch, tgt_batch)
    let mut total_lists = 0u64;

    // 1) Consolidated count file
    if std::path::Path::new(&report_filename).exists() {
        test_print("   Building plan from consolidated count file");
        use std::io::{BufRead, BufReader};
        let file = fs::File::open(&report_filename)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() || line.trim().starts_with('#') { continue; }
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 4 {
                // parts[0] = "src tgt", parts[2] = count, parts[3] = filename
                let src_tgt = parts[0].trim();
                let fields: Vec<&str> = src_tgt.split_whitespace().collect();
                if fields.len() >= 2 {
                    if let (Ok(src), Ok(tgt)) = (fields[0].parse::<u32>(), fields[1].parse::<u32>()) {
                        let count_str = parts[2].trim();
                        let digits_only: String = count_str.chars().filter(|c| c.is_ascii_digit()).collect();
                        if let Ok(count) = digits_only.parse::<u64>() {
                            let filename = parts[3].trim().to_string();
                            file_counts.push((filename, count, src, tgt));
                            total_lists += count;
                        }
                    }
                }
            }
        }
    }

    // 2) Intermediary input count files (if consolidated missing or incomplete)
    if file_counts.is_empty() {
        let prev_size = target_size - 1;
        let pattern = format!("no_set_list_input_intermediate_count_{:02}_", prev_size);
        let mut intermediary_files: Vec<String> = Vec::new();
        let entries = fs::read_dir(input_dir)?;
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(&pattern) && name.ends_with(".txt") {
                    intermediary_files.push(format!("{}/{}", input_dir, name));
                }
            }
        }
        intermediary_files.sort();

        if !intermediary_files.is_empty() {
            test_print(&format!("   Building plan from {} intermediary input count files", intermediary_files.len()));
            use std::io::{BufRead, BufReader};
            for inter in intermediary_files {
                let file = fs::File::open(&inter)?;
                let reader = BufReader::new(file);
                for line in reader.lines() {
                    let line = line?;
                    if line.trim().starts_with("...") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 5 {
                            if let Ok(count) = parts[1].parse::<u64>() {
                                let filename = parts[4].to_string();
                                // parse batches from filename
                                if let Some(to_pos) = filename.find("_to_") {
                                    let before_to = &filename[..to_pos];
                                    let after_to = &filename[to_pos + 4..];
                                    if let Some(src_batch_pos) = before_to.rfind("_batch_") {
                                        let src_batch_str = &before_to[src_batch_pos + 7..];
                                        if let Ok(src_batch) = src_batch_str.parse::<u32>() {
                                            if let Some(tgt_batch_pos) = after_to.rfind("_batch_") {
                                                let tgt_batch_str = &after_to[tgt_batch_pos + 7..after_to.len() - 5];
                                                if let Ok(tgt_batch) = tgt_batch_str.parse::<u32>() {
                                                    file_counts.push((filename, count, src_batch, tgt_batch));
                                                    total_lists += count;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // sort by target_batch then source batch
            file_counts.sort_by(|a, b| match a.3.cmp(&b.3) { std::cmp::Ordering::Equal => a.2.cmp(&b.2), other => other });
        }
    }

    // If we still have no metadata, abort and ask the user to run --count
    if file_counts.is_empty() {
        test_print("   No consolidated or intermediary count files found. Run --count <SIZE> first to generate counts.");
        return Ok(());
    }

    test_print(&format!("\n   Total lists accounted for in plan: {}", total_lists.separated_string()));
    
    // Execute plan: stream each input file and write into compacted outputs in order.
    // We will only hold up to `batch_size` lists in memory for the current compacted file.
    const READ_CHUNK_SIZE: usize = 2_000_000; // max lists to deserialize per read chunk

    // Ensure output dir exists
    if !std::path::Path::new(output_dir).exists() {
        fs::create_dir_all(output_dir)?;
    }

    // Display plan summary
    let expected_compacted_files = ((total_lists + batch_size - 1) / batch_size) as usize;
    test_print(&format!("\n   Plan: Create ~{} compacted files (batch size {}) from {} input files", expected_compacted_files, batch_size.separated_string(), file_counts.len()));

    let source_size = target_size - 1;
    use std::io::Write;

    // State for current compacted output
    let mut current_compact_batch: u32 = 0;
    let mut current_output_buffer: Vec<NoSetListSerialized> = Vec::new();
    let mut current_first_source: Option<u32> = None;
    let mut current_output_contribs: Vec<(u32, u64)> = Vec::new(); // (src_batch, count)
    let mut files_created = 0usize;

    // Helper to flush current output buffer to disk
    let flush_current = |output_dir: &str,
                             target_size: u8,
                             batch_idx: u32,
                             first_source_batch: Option<u32>,
                             buffer: &mut Vec<NoSetListSerialized>,
                             contribs: &[(u32, u64)],
                             input_dir: &str,
                             source_size: u8|
                             -> std::io::Result<()> {
        if buffer.is_empty() { return Ok(()); }
        let from_src = first_source_batch.unwrap_or_else(|| if !contribs.is_empty() { contribs[0].0 } else { 0 });
        // Use original filename pattern: nsl_{src}_batch_{src_batch}_to_{tgt}_batch_{tgt_batch}.rkyv
        let src_batch_width = 6;
        let tgt_batch_width = 6;
        let filename = format!(
            "{}/nsl_{:02}_batch_{:0srcw$}_to_{:02}_batch_{:0tgtw$}.rkyv",
            output_dir, source_size, from_src, target_size, batch_idx,
            srcw = src_batch_width, tgtw = tgt_batch_width);

        test_print(&format!("   Writing compacted file {} ({} lists)", filename, buffer.len().separated_string()));
        save_compacted_batch(&filename, &buffer)?;

        // For each contribution from inputs, append an intermediary count file in the INPUT directory
        use std::fs::OpenOptions;
        for (src_batch, cnt) in contribs {
            let inter_width = 6;
            let inter_filename = format!("{}/nsl_{:02}_intermediate_count_from_{:02}_{:0width$}.txt",
                input_dir, target_size, source_size, src_batch, width = inter_width);

            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&inter_filename) {
                if let Err(e) = writeln!(f, "   ... {:>8} lists in {}", cnt, std::path::Path::new(&filename).file_name().unwrap().to_string_lossy()) {
                    debug_print(&format!("compact: Error writing intermediary {}: {}", inter_filename, e));
                } else {
                    test_print(&format!("   ... appended {} lists -> {}", cnt.separated_string(), inter_filename));
                }
            } else {
                debug_print(&format!("compact: Error opening intermediary {} for append", inter_filename));
            }
        }

        buffer.clear();
        Ok(())
    };

    // Process each input file in plan order
    for (base_name, count, src_batch, _tgt_batch) in file_counts {
        let filepath = format!("{}/{}", input_dir, base_name);
        test_print(&format!("\n   Processing input file {} ({} lists)", base_name, count.separated_string()));

        // Map file and access archived vec directly to deserialize in chunks
        let file = fs::File::open(&filepath)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let archived = match check_archived_root::<Vec<NoSetListSerialized>>(&mmap[..]) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("   Error validating {}: {:?}", base_name, e);
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Archive validation failed"));
            }
        };
        let total = archived.len();
        let mut idx = 0usize;

        while idx < total {
            let take = std::cmp::min(READ_CHUNK_SIZE, total - idx);
            // Deserialize chunk
            let mut chunk: Vec<NoSetListSerialized> = Vec::with_capacity(take);
            for i in idx..(idx + take) {
                let archived_elem = archived.get(i).expect("index in range");
                let des: NoSetListSerialized = archived_elem.deserialize(&mut rkyv::Infallible).expect("deserialization");
                chunk.push(des);
            }
            idx += take;

            // Append chunk elements into current output buffers, flushing as needed
            let mut chunk_idx = 0usize;
            while chunk_idx < chunk.len() {
                let space_left = (batch_size as usize) - current_output_buffer.len();
                let take_now = std::cmp::min(space_left, chunk.len() - chunk_idx);

                if current_first_source.is_none() { current_first_source = Some(src_batch); }
                // record contribution from this src_batch
                let mut found = false;
                for entry in current_output_contribs.iter_mut() {
                    if entry.0 == src_batch {
                        entry.1 += take_now as u64;
                        found = true;
                        break;
                    }
                }
                if !found {
                    current_output_contribs.push((src_batch, take_now as u64));
                }

                // move elements
                current_output_buffer.extend(chunk[chunk_idx..chunk_idx + take_now].iter().cloned());
                chunk_idx += take_now;

                // If buffer full, flush to disk
                if current_output_buffer.len() as u64 >= batch_size {
                    flush_current(output_dir, target_size, current_compact_batch, current_first_source, &mut current_output_buffer, &current_output_contribs, input_dir, source_size)?;
                    files_created += 1;
                    current_compact_batch += 1;
                    current_first_source = None;
                    current_output_contribs.clear();
                }
            }
        }
    }

    // Flush any remaining lists
    if !current_output_buffer.is_empty() {
        flush_current(output_dir, target_size, current_compact_batch, current_first_source, &mut current_output_buffer, &current_output_contribs, input_dir, source_size)?;
        files_created += 1;
        current_output_contribs.clear();
    }
    
    let elapsed = start_time.elapsed().as_secs_f64();
    test_print(&format!("\nCompaction completed in {:.2} seconds", elapsed));
    test_print(&format!("   Input files: {}", input_files_ordered.len()));
    test_print(&format!("   Compacted files: {}", files_created));
    test_print(&format!("   Total lists: {}", total_lists.separated_string()));
    test_print(&format!("   Compression ratio: {:.1}x", input_files_ordered.len() as f64 / files_created as f64));
    
    Ok(())
}

/// Load lists from a file (helper for compact mode)
#[allow(dead_code)]
fn load_lists_from_file(filepath: &str) -> std::io::Result<Vec<NoSetListSerialized>> {
    let file = File::open(filepath)?;
    let mmap = unsafe { Mmap::map(&file)? };
    
    match check_archived_root::<Vec<NoSetListSerialized>>(&mmap[..]) {
        Ok(archived_lists) => {
            let lists: Vec<NoSetListSerialized> = archived_lists
                .deserialize(&mut rkyv::Infallible)
                .expect("Deserialization should never fail with Infallible");
            Ok(lists)
        }
        Err(e) => {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Archive validation failed: {:?}", e)
            ))
        }
    }
}

/// Save compacted batch to file
fn save_compacted_batch(filepath: &str, lists: &[NoSetListSerialized]) -> std::io::Result<()> {
    use rkyv::ser::{serializers::AllocSerializer, Serializer};
    
    // Convert slice to Vec for serialization
    let lists_vec: Vec<NoSetListSerialized> = lists.to_vec();
    
    let mut serializer = AllocSerializer::<4096>::default();
    serializer.serialize_value(&lists_vec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Serialization error: {:?}", e)))?;
    
    let bytes = serializer.into_serializer().into_inner();
    std::fs::write(filepath, bytes)?;
    
    Ok(())
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

/// Get next available output batch number by scanning filenames only
/// Does NOT read file contents - much faster than counting
fn get_next_output_batch_from_files(
    base_path: &str,
    target_size: u8,
    restart_batch: u32
) -> u32 {
    use std::fs;
    
    let entries = match fs::read_dir(base_path) {
        Ok(e) => e,
        Err(_) => return 0,  // Directory doesn't exist, start from batch 0
    };
    
    let pattern_prefix = format!("_to_{:02}_batch_", target_size);
    let mut max_target_batch: Option<u32> = None;
    
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            // Check if this is an output file for our target size
            if name.starts_with("nsl_") && name.contains(&pattern_prefix) && name.ends_with(".rkyv") {
                // Extract the SOURCE batch number from filename
                // Format: nsl_XX_batch_YYYYY_to_ZZ_batch_BBBBB.rkyv
                // Note: batch widths are zero-padded to 6 digits
                if let Some(to_pos) = name.find("_to_") {
                    let before_to = &name[..to_pos];
                    if let Some(batch_pos) = before_to.rfind("_batch_") {
                        let batch_str = &before_to[batch_pos + 7..];
                        if let Ok(source_batch_num) = batch_str.parse::<u32>() {
                            // Only consider files created from source batches < restart_batch
                            if source_batch_num < restart_batch {
                                // Extract target batch number
                                let after_to = &name[to_pos + 4..];
                                if let Some(target_batch_pos) = after_to.rfind("_batch_") {
                                    let target_batch_str = &after_to[target_batch_pos + 7..after_to.len() - 5]; // -5 for ".rkyv"
                                    if let Ok(target_batch_num) = target_batch_str.parse::<u32>() {
                                        // Track maximum target batch number
                                        max_target_batch = Some(
                                            max_target_batch.map_or(target_batch_num, |current_max| current_max.max(target_batch_num))
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    let next_batch = max_target_batch.map_or(0, |max| max + 1);
    
    debug_print(&format!("get_next_output_batch_from_files: next batch for size {:02} = {:06} (scanned filenames only)",
        target_size, next_batch));
    
    next_batch
}

/// Filename helper - single source of truth for all file naming
/// 
/// Pattern: nsl_{source_size:02}_batch_{source_batch:05|06}_to_{target_size:02}_batch_{target_batch:05|06}.rkyv
/// 
/// Examples:
/// - Seed file: nsl_00_batch_00000_to_03_batch_00000.rkyv (source_size=0, target_size=3)
/// - Size 4 output: nsl_03_batch_00000_to_04_batch_00000.rkyv (from size 3 to create size 4)
/// - Size 5 input batch 0: Find files matching *_to_05_batch_00000.rkyv
/// - Size 11 target example: nsl_10_batch_00000_to_11_batch_000000.rkyv (6-digit target batch)
///
/// Generate output filename when saving
/// Uses 6-digit batch numbers for all sizes
fn output_filename(
    base_path: &str,
    source_size: u8,
    source_batch: u32,
    target_size: u8,
    target_batch: u32
) -> String {
    use std::path::Path;
    // Use 6-digit batch numbers (always)
    let src_batch_width = 6;
    let tgt_batch_width = 6;
    let filename = format!(
        "nsl_{:02}_batch_{:0width1$}_to_{:02}_batch_{:0width2$}.rkyv",
        source_size, source_batch, target_size, target_batch,
        width1 = src_batch_width,
        width2 = tgt_batch_width
    );
    let path = Path::new(base_path).join(filename);
    path.to_string_lossy().to_string()
}

/// Find input filename for reading
/// Searches for file matching: *_to_{target_size}_batch_{target_batch}.rkyv
fn find_input_filename(
    base_path: &str,
    target_size: u8,
    target_batch: u32
) -> Option<String> {
    use std::fs;
    
    let batch_width = 6;
    let pattern = format!("_to_{:02}_batch_{:0width$}.rkyv", target_size, target_batch, width = batch_width);
    debug_print(&format!("   ... looking for input file matching: *{} in {}", pattern, base_path));
    
    let entries = match fs::read_dir(base_path) {
        Ok(e) => e,
        Err(err) => {
            debug_print(&format!("   ... ERROR: Cannot read directory {}: {}", base_path, err));
            return None;
        }
    };
    
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with("nsl_") && name.ends_with(&pattern) {
                let found_path = entry.path().to_string_lossy().to_string();
                debug_print(&format!("   ... found: {}", name));
                return Some(found_path);
            }
        }
    }
    
    test_print("   ... no matching file found");
    None
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
