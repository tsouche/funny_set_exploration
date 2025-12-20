/// Version 0.4.13: Hybrid stack-optimized computation with auto-compaction for sizes 13+
/// Added: Cascade mode for automated multi-size processing
/// 
/// This implementation combines the best of both worlds:
/// - Uses NoSetList (stack arrays) for computation → 4-5× faster
/// - Converts to NoSetListSerialized (heap Vecs) for I/O → compact 2GB files
/// - GlobalFileState with incremental JSON/TXT saves after each output file
/// - Recognizes both regular and compacted input files (*_compacted.rkyv)
/// - Supports batch range processing for smart compaction workflows
/// 
/// Performance characteristics:
/// - Computation: Same speed as v0.3.0 (stack-optimized)
/// - File size: ~2GB per 20M batch (compact with size_32 rkyv)
/// - Memory: Moderate (~12-15GB peak during conversion + save)
/// - Tracking: In-memory state with O(1) lookups, atomic JSON/TXT persistence
///
/// This is the only active version of the project.

// Rkyv imports for zero-copy serialization
use rkyv::check_archived_root;

use separator::Separatable;
use crate::utils::*;
use crate::set::*;
use crate::no_set_list::*;
use crate::io_helpers::*;
use crate::filenames::*;
use crate::file_info::GlobalFileState;

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
    fn save_new_to_file(&mut self, state: Option<&mut GlobalFileState>) -> bool {
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

                // Register in state or buffer for legacy intermediary file
                if let Some(state) = state {
                    let file_path = std::path::Path::new(&file);
                    let (file_size, mtime) = file_path.metadata()
                        .ok()
                        .map(|m| (
                            Some(m.len()),
                            m.modified().ok()
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs() as i64)
                        ))
                        .unwrap_or((None, None));
                    
                    let filename = file_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&file)
                        .to_string();
                    
                    state.register_file(
                        &filename,
                        self.current_file_batch,
                        self.new_output_batch,
                        additional_new,
                        false,
                        file_size,
                        mtime,
                    );
                    
                    // Flush state immediately after saving each output file
                    if let Err(e) = state.flush() {
                        debug_print(&format!("Error flushing global state: {}", e));
                    }
                } else {
                    // Fallback to legacy buffer system
                    self.buffer_input_intermediary_line(self.new_output_batch, additional_new);
                }
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
    fn process_one_file_of_current_size_n(&mut self, max: &u64, mut state: Option<&mut GlobalFileState>) -> u64 {
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
                if !self.save_new_to_file(state.as_deref_mut()) {
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
            if !self.save_new_to_file(state.as_deref_mut()) {
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
    fn process_batch_loop(&mut self, max: &u64, stop_after_one: bool, mut state: Option<&mut GlobalFileState>) -> u32 {
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

                self.process_one_file_of_current_size_n(max, state.as_deref_mut());

                // Write legacy intermediary file only if not using state
                if state.is_none() {
                    let batch_width = 6;
                    let intermediary_filename = format!(
                        "no_set_list_input_intermediate_count_{:02}_{:0width$}.txt",
                        self.current_size, self.current_file_batch,
                        width = batch_width
                    );
                    self.write_input_intermediary_file();
                    test_print(&format!("   ... saving input intermediary file {}", intermediary_filename));
                }
                batches_processed += 1;
                
                // Increment batch counter to move to next input file
                self.current_file_batch += 1;
                
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
    pub fn process_all_files_of_current_size_n(&mut self, current_size: u8, max: &u64, state: Option<&mut GlobalFileState>) -> u64 {
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
        self.process_batch_loop(max, false, state);
        
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
    pub fn process_from_batch(&mut self, current_size: u8, start_batch: u32, max: &u64, state: Option<&mut GlobalFileState>) -> u64 {
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
        self.process_batch_loop(max, false, state);
        
        debug_print(&format!("process_from_batch: Finished processing size {:02} from batch {}", 
            self.current_size, start_batch));
        
        // Report results
        let elapsed_secs = start_time.elapsed().as_secs_f64();
        created_a_total_of(self.new_total_list_count, self.current_size + 1, elapsed_secs);
        self.print_timing_report(start_time);
        
        self.new_total_list_count
    }
    
    /// Process files within a specific batch range (inclusive)
    /// Used when we want to limit processing to a specific range (e.g., only compacted files)
    pub fn process_batch_range(&mut self, current_size: u8, start_batch: u32, end_batch: u32, max: &u64, mut state: Option<&mut GlobalFileState>) -> u64 {
        if current_size < 3 {
            debug_print("process_batch_range: size must be >= 3");
            return 0;
        }
        
        debug_print(&format!("process_batch_range: processing no-set-{:02} from batch {} to {}", 
            current_size, start_batch, end_batch));
        
        let start_time = std::time::Instant::now();
        
        // Initialize from specific batch
        self.init_processing_state(current_size, start_batch);
        self.init_output_batch(start_batch);  // Scan for next available output batch
        
        // Process batches in the range [start_batch, end_batch]
        let mut batches_processed = 0u64;
        for batch in start_batch..=end_batch {
            self.current_file_batch = batch;
            
            // Add blank line before loading next batch (except for the first one)
            if batches_processed > 0 {
                test_print("");
            }
            test_print(&format!("   ... loading batch {}", self.current_file_batch));
            
            // Try to load this batch
            if self.refill_current_from_file() {
                test_print(&format!("   ... loaded {:>10} lists from batch {}", 
                    self.current.len().separated_string(), self.current_file_batch));
                
                // Process the cards and create new lists
                self.process_one_file_of_current_size_n(max, state.as_deref_mut());
                batches_processed += 1;
            } else {
                // File not found - this could be normal if some batches don't exist
                test_print(&format!("   ... Batch {:06} not found, skipping", batch));
            }
        }
        
        debug_print(&format!("process_batch_range: Finished processing size {:02} batches {} to {} ({} batches processed)", 
            self.current_size, start_batch, end_batch, batches_processed));
        
        // Report results
        let elapsed_secs = start_time.elapsed().as_secs_f64();
        created_a_total_of(self.new_total_list_count, self.current_size + 1, elapsed_secs);
        self.print_timing_report(start_time);
        
        self.new_total_list_count
    }
    
    /// Process a single input batch (unitary processing)
    /// Processes one specific input file and generates its output files
    pub fn process_single_batch(&mut self, input_size: u8, input_batch: u32, max: &u64, state: Option<&mut GlobalFileState>) -> u64 {
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
        let batches_processed = self.process_batch_loop(max, true, state);
        
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
/// 
/// Count files for a given target size and create summary report
/// 
/// This function uses an input-intermediary file system:
/// 1. Input-intermediary files: nsl_{target_size:02}_intermediate_count_from_{input_size:02}_{input_batch:06}.txt
///    - Created automatically during file generation (--size, --unitary modes and --compact)
///    - One file per input batch, tracks which output files include lists from that input batch
/// 
/// File naming:
/// - Input-intermediary: nsl_{target_size:02}_intermediate_count_from_{input_size:02}_{input_batch:06}.txt
/// - Final report: nsl_{target_size:02}_global_count.txt
/// 
/// All files are stored in the same directory as the source files (base_path)
pub fn count_size_files(base_path: &str, target_size: u8, force: bool, _keep_state: bool) -> std::io::Result<()> {
    use std::fs;
    use std::path::PathBuf;
    
    test_print(&format!("\nCounting files for size {:02}...", target_size));
    test_print(&format!("   Input directory: {}", base_path));
    // Count mode: reads existing input-intermediary files named
    // `nsl_{target_size:02}_intermediate_count_from_{source_size:02}_{input_batch:06}.txt`
    // and consolidates them into the final `nsl_{size:02}_global_count.txt` report.
    // It no longer creates or updates these small intermediary files; they must be present.
    
    let start_time = std::time::Instant::now();
    
    // Step 1: Scan for all .rkyv files in directory
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
    
    // Step 2: Load or create GlobalFileState
    use std::collections::HashSet;
    use crate::file_info::GlobalFileState;
    
    let mut state = if !force {
        // Try to load existing global info (JSON/rkyv or txt)
        match GlobalFileState::from_sources(base_path, target_size) {
            Ok(existing_state) => {
                test_print("   ... Loading existing global info file...");
                let file_count = existing_state.entries().len();
                test_print(&format!("   ... Loaded {} files from existing global info", file_count));
                existing_state
            }
            Err(e) => {
                test_print(&format!("   ... Could not load existing global info: {}", e));
                test_print("   ... Creating new state...");
                GlobalFileState::new(base_path, target_size)
            }
        }
    } else {
        test_print("   ... FORCE mode: Creating new state from scratch...");
        GlobalFileState::new(base_path, target_size)
    };
    
    // Build set of files already in state
    let mut seen_files: HashSet<String> = state.entries().keys()
        .map(|(_, _, filename)| filename.clone())
        .collect();
    
    // Step 3: Scan directory for .rkyv files not in state and add them
    test_print(&format!("   ... Scanning directory for files not in state..."));
    let mut files_added = 0;
    let mut files_counted = 0;
    
    for path in &all_files {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let filename = name.to_string();
            
            // Skip if already in state
            if seen_files.contains(&filename) {
                continue;
            }
            
            files_counted += 1;
            if files_counted % 100 == 0 {
                progress_print(&format!("   ... Processed {} new files...", files_counted));
            }
            
            // Parse batch numbers from filename
            if let Some(to_pos) = name.find("_to_") {
                let before_to = &name[..to_pos];
                let after_raw = &name[to_pos + 4..];
                let after_to = if let Some(stripped) = after_raw.strip_suffix("_compacted.rkyv") {
                    stripped
                } else if let Some(stripped) = after_raw.strip_suffix(".rkyv") {
                    stripped
                } else {
                    after_raw
                };
                
                if let Some(src_batch_pos) = before_to.rfind("_batch_") {
                    let src_batch_str = &before_to[src_batch_pos + 7..];
                    if let Ok(src_batch) = src_batch_str.parse::<u32>() {
                        if let Some(tgt_batch_pos) = after_to.rfind("_batch_") {
                            let tgt_batch_str = &after_to[tgt_batch_pos + 7..];
                            if let Ok(tgt_batch) = tgt_batch_str.parse::<u32>() {
                                // Count lists in this file
                                use memmap2::Mmap;
                                if let Ok(file) = fs::File::open(path) {
                                    if let Ok(mmap) = unsafe { Mmap::map(&file) } {
                                        if let Ok(arch) = check_archived_root::<Vec<NoSetListSerialized>>(&mmap[..]) {
                                            let count = arch.len() as u64;
                                            let is_compacted = name.contains("_compacted.rkyv");
                                            
                                            // Get file metadata
                                            let (file_size, mtime) = path.metadata()
                                                .ok()
                                                .map(|m| (
                                                    Some(m.len()),
                                                    m.modified().ok()
                                                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                                        .map(|d| d.as_secs() as i64)
                                                ))
                                                .unwrap_or((None, None));
                                            
                                            // Add to state
                                            state.register_file(
                                                &filename,
                                                src_batch,
                                                tgt_batch,
                                                count,
                                                is_compacted,
                                                file_size,
                                                mtime
                                            );
                                            
                                            seen_files.insert(filename.clone());
                                            files_added += 1;
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
    
    if files_added > 0 {
        test_print(&format!("   ... Added {} new files to state", files_added));
    } else {
        test_print("   ... No new files to add, state is up to date");
    }

    // Helper to display processed batches in compact groups (10 per line)
    
    // Step 4: Save updated state (rkyv, JSON, and TXT)
    test_print(&format!("\n   ... Saving state with {} files...", state.entries().len()));
    state.flush()?;
    
    // Export human-readable formats
    state.export_human_readable()?;
    
    let elapsed = start_time.elapsed().as_secs_f64();
    test_print(&format!("\nCount completed in {:.2} seconds", elapsed));
    test_print(&format!("State saved to: {}/nsl_{:02}_global_info.rkyv", base_path, target_size));
    test_print(&format!("Exported to: {}/nsl_{:02}_global_info.json and .txt", base_path, target_size));
    Ok(())
}

/// Check if an intermediary file is valid (exists and is newer than all source files)
/// Validate an input-intermediary file for a given input batch
/// Checks:
/// 1. File exists
/// 2. File's timestamp is more recent than the source .rkyv file
/// 3. File contains an entry for the source .rkyv file
fn _is_intermediary_file_valid(intermediary_file: &str, source_files: &[std::path::PathBuf]) -> std::io::Result<bool> {
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
/// Helper: create input-intermediary files from a list of .rkyv files (one per source batch)
fn _create_input_intermediary_from_files(files: &[std::path::PathBuf], output_file: &str) -> std::io::Result<u64> {
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

/// Helper to print large numbers with thousand separators and timing info
pub fn created_a_total_of(nb: u64, size: u8, elapsed_secs: f64) {
        let hours = (elapsed_secs / 3600.0) as u64;
        let minutes = ((elapsed_secs % 3600.0) / 60.0) as u64;
        let seconds = (elapsed_secs % 60.0) as u64;
        
        test_print(&format!("   ... created a total of {:>15} no-set-{:02} lists \
            in {:>10.2} seconds ({:02}h{:02}m{:02}s)", 
            nb.separated_string(), size, elapsed_secs, hours, minutes, seconds));
    }



#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;

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

        // Verify that a consolidated global report is created
        let report = base.join(format!("nsl_{:02}_global_count.txt", target_size));
        assert!(report.exists());

        let before = fs::read_to_string(&report).unwrap();
        let before_lines = before.lines().count();
        assert!(before_lines >= 3);

        // Run count second time; it should not duplicate entries in the global report
        count_size_files(base.to_str().unwrap(), target_size, false, true).unwrap();
        let after = fs::read_to_string(&report).unwrap();
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

}

/// Regenerate the consolidated global report from the partial CSV file.
    /// This is called after each processed batch so the human-readable global report
    /// is up-to-date and reflects progress mid-run.
pub fn _regenerate_report_from_partial(base_path: &str, target_size: u8, partial_filename: &str, intermediary_files_total: usize) -> std::io::Result<()> {
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
    return Ok(());
}

/// Compact small output files into larger 10M-entry batches
/// Delegates to the `compaction` module which implements idempotent, atomic compaction.
pub fn compact_size_files(input_dir: &str, output_dir: &str, target_size: u8, batch_size: u64, max_batch: Option<u32>) -> std::io::Result<()> {
    crate::compaction::compact_size_files(input_dir, output_dir, target_size, batch_size, max_batch)
}

/// Save compacted batch to file
#[allow(dead_code)]
fn save_compacted_batch(filepath: &str, lists: &[NoSetListSerialized]) -> std::io::Result<()> {
    use rkyv::ser::{serializers::AllocSerializer, Serializer};
    
    let lists_vec: Vec<NoSetListSerialized> = lists.to_vec();
    
    let mut serializer = AllocSerializer::<4096>::default();
    serializer.serialize_value(&lists_vec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Serialization error: {:?}", e)))?;
    
    let bytes = serializer.into_serializer().into_inner();
    std::fs::write(filepath, bytes)?;
    
    Ok(())
}

