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
                debug_print(&format!("   ... No input file found for size {:02} batch {:05} in {}",
                    self.current_size, self.current_file_batch, self.input_path));
                debug_print(&format!("refill_current_from_file: No file found for size {:02} batch {:05}",
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
            test_print(&format!("   ... saving final batch ({} lists), output batch {}\n", 
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
        debug_print(&format!("   ... processed {} input lists, created {} new lists from batch {:05}",
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
            test_print(&format!("   ... loading batch {}", self.current_file_batch));
            let loaded = self.refill_current_from_file();
            
            if loaded {
                test_print(&format!("   ... loaded {:>10} lists from batch {}", 
                    self.current.len().separated_string(), self.current_file_batch));
                self.process_one_file_of_current_size_n(max);
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
        
        debug_print(&format!("process_single_batch: processing input size {:02} batch {:05}", 
            input_size, input_batch));
        
        let start_time = std::time::Instant::now();
        
        // Initialize for single batch
        self.init_processing_state(input_size, input_batch);
        self.init_output_batch(input_batch);  // Scan for next available output batch
        
        test_print(&format!("   ... will create output starting from batch {:05}", self.new_output_batch));
        
        // Process only this one batch
        let batches_processed = self.process_batch_loop(max, true);
        
        if batches_processed == 0 {
            test_print(&format!("   ... ERROR: Could not load input file for size {:02} batch {:05}",
                input_size, input_batch));
            return 0;
        }
        
        debug_print(&format!("process_single_batch: Finished processing size {:02} batch {:05}", 
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
/// File naming:
/// - Intermediary files: no_set_list_intermediate_count_{target_size:02}_{batch_idx:04}.txt
/// - Final report: no_set_list_count_{target_size:02}.txt
/// 
/// Both are stored in the same directory as the source files (input_path)
pub fn count_size_files(base_path: &str, target_size: u8) -> std::io::Result<()> {
    use std::fs;
    use std::path::PathBuf;
    
    test_print(&format!("\nCounting files for size {:02}...", target_size));
    test_print(&format!("   Input directory: {}", base_path));
    
    let start_time = std::time::Instant::now();
    
    // Collect all matching files
    let entries = fs::read_dir(base_path)?;
    let pattern = format!("_to_{:02}_batch_", target_size);
    
    let mut all_files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            // Match both regular files (nsl_XX_batch_*) and seed files (nsl_00_batch_*)
            if name.starts_with("nsl_") && name.contains(&pattern) && name.ends_with(".rkyv") {
                all_files.push(entry.path());
            }
        }
    }
    
    if all_files.is_empty() {
        test_print(&format!("   ... No files found for size {:02}", target_size));
        return Ok(());
    }
    
    // Sort files to ensure consistent batch composition across runs
    all_files.sort();
    
    test_print(&format!("   ... Found {} files to count", all_files.len()));
    
    // Process files in batches
    let num_batches = (all_files.len() + COUNT_BATCH_SIZE - 1) / COUNT_BATCH_SIZE;
    test_print(&format!("   ... Processing in {} batches of up to {} files each", num_batches, COUNT_BATCH_SIZE));
    
    let mut intermediary_files = Vec::new();
    let mut batches_skipped = 0usize;
    let mut batches_processed = 0usize;
    
    for (batch_idx, chunk) in all_files.chunks(COUNT_BATCH_SIZE).enumerate() {
        let intermediary_filename = format!("{}/no_set_list_intermediate_count_{:02}_{:04}.txt", 
            base_path, target_size, batch_idx);
        
        // Check if intermediary file exists and is up-to-date
        if is_intermediary_file_valid(&intermediary_filename, chunk)? {
            test_print(&format!("\n   ... Batch {:04}/{}: Skipping (intermediary file is up-to-date)", 
                batch_idx, num_batches));
            batches_skipped += 1;
        } else {
            test_print(&format!("\n   ... Batch {:04}/{}: Processing {} files...", 
                batch_idx, num_batches, chunk.len()));
            
            process_count_batch(chunk, &intermediary_filename, target_size)?;
            batches_processed += 1;
        }
        
        intermediary_files.push(intermediary_filename);
    }
    
    // Consolidate intermediary files into final report
    test_print(&format!("\n   ... Consolidating {} intermediary files...", intermediary_files.len()));
    
    consolidate_count_files(&intermediary_files, base_path, target_size)?;
    
    // Keep intermediary files for idempotency (don't delete them)
    test_print("   ... Intermediary files kept for future idempotent runs");
    
    let elapsed = start_time.elapsed().as_secs_f64();
    test_print(&format!("\nCount completed in {:.2} seconds", elapsed));
    test_print(&format!("   Batches processed: {}", batches_processed));
    test_print(&format!("   Batches skipped (up-to-date): {}", batches_skipped));
    
    Ok(())
}

/// Check if an intermediary file is valid (exists and is newer than all source files)
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
fn process_count_batch(files: &[std::path::PathBuf], output_file: &str, _target_size: u8) -> std::io::Result<()> {
    use std::fs;
    use std::io::Write;
    use std::collections::BTreeMap;
    
    // Key: (source_batch, target_batch), Value: (filename, count)
    let mut file_info: BTreeMap<(u32, u32), (String, u64)> = BTreeMap::new();
    
    for path in files {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Parse source and target batch numbers
            // Format: nsl_XX_batch_YYYYY_to_ZZ_batch_BBBBB.rkyv
            if let Some(to_pos) = name.find("_to_") {
                let before_to = &name[..to_pos];
                let after_to = &name[to_pos + 4..];
                
                // Extract source batch
                if let Some(src_batch_pos) = before_to.rfind("_batch_") {
                    let src_batch_str = &before_to[src_batch_pos + 7..];
                    if let Ok(source_batch) = src_batch_str.parse::<u32>() {
                        // Extract target batch
                        if let Some(tgt_batch_pos) = after_to.rfind("_batch_") {
                            let tgt_batch_str = &after_to[tgt_batch_pos + 7..after_to.len() - 5]; // -5 for ".rkyv"
                            if let Ok(target_batch) = tgt_batch_str.parse::<u32>() {
                                // Read file and count entries
                                if let Some(vec_nlist) = read_from_file_serialized(path.to_str().unwrap()) {
                                    let count = vec_nlist.len() as u64;
                                    file_info.insert((source_batch, target_batch), (name.to_string(), count));
                                    test_print(&format!("      ... {:>10} lists in {}",
                                        count.separated_string(), name));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Write intermediary file (format: "   ... count lists in filename")
    let mut file = fs::File::create(output_file)?;
    for ((_source_batch, _target_batch), (filename, count)) in file_info.iter() {
        writeln!(file, "   ... {:>8} lists in {}", count, filename)?;
    }
    
    Ok(())
}

/// Consolidate all intermediary count files into the final report
fn consolidate_count_files(intermediary_files: &[String], base_path: &str, target_size: u8) -> std::io::Result<()> {
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::collections::BTreeMap;
    
    // Key: (source_batch, target_batch), Value: (filename, count)
    let mut all_file_info: BTreeMap<(u32, u32), (String, u64)> = BTreeMap::new();
    
    // Read all intermediary files
    for intermediary_file in intermediary_files {
        let file = fs::File::open(intermediary_file)?;
        let reader = BufReader::new(file);
        
        for line in reader.lines() {
            let line = line?;
            // Format: "   ... count lists in filename"
            if line.trim().starts_with("...") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                // parts = ["...", count, "lists", "in", filename]
                if parts.len() >= 5 {
                    if let Ok(count) = parts[1].parse::<u64>() {
                        let filename = parts[4];
                        // Parse source and target batch numbers from filename
                        if let Some(to_pos) = filename.find("_to_") {
                            let before_to = &filename[..to_pos];
                            let after_to = &filename[to_pos + 4..];
                            
                            if let Some(src_batch_pos) = before_to.rfind("_batch_") {
                                let src_batch_str = &before_to[src_batch_pos + 7..];
                                if let Ok(src_batch) = src_batch_str.parse::<u32>() {
                                    if let Some(tgt_batch_pos) = after_to.rfind("_batch_") {
                                        let tgt_batch_str = &after_to[tgt_batch_pos + 7..after_to.len() - 5]; // -5 for ".rkyv"
                                        if let Ok(tgt_batch) = tgt_batch_str.parse::<u32>() {
                                            all_file_info.insert((src_batch, tgt_batch), (filename.to_string(), count));
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
    
    // Create final summary report file
    let report_filename = format!("{}/no_set_list_count_{:02}.txt", base_path, target_size);
    let mut report_file = fs::File::create(&report_filename)?;
    
    // Write header
    writeln!(report_file, "# File Count Summary for no-set-{:02} lists", target_size)?;
    writeln!(report_file, "# Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))?;
    writeln!(report_file, "# Input directory: {}", base_path)?;
    writeln!(report_file, "# Intermediary files used: {} batch files", intermediary_files.len())?;
    writeln!(report_file, "# Format: source_batch target_batch | cumulative_nb_lists | nb_lists_in_file | filename")?;
    writeln!(report_file, "#")?;
    
    // Sort by target_batch (ascending), then source_batch (ascending) for cumulative calculation
    let mut sorted_files: Vec<_> = all_file_info.iter().collect();
    sorted_files.sort_by(|a, b| {
        match a.0.1.cmp(&b.0.1) { // target_batch ascending
            std::cmp::Ordering::Equal => a.0.0.cmp(&b.0.0), // source_batch ascending
            other => other,
        }
    });
    
    // Calculate cumulative totals (from lowest to highest batch)
    let mut cumulative = 0u64;
    let mut report_lines = Vec::new();
    
    for ((source_batch, target_batch), (filename, count)) in sorted_files.iter() {
        cumulative += count;
        report_lines.push(format!(
            "{:05} {:05} | {:>15} | {:>15} | {}",
            source_batch,
            target_batch,
            cumulative.separated_string(),
            count.separated_string(),
            filename
        ));
    }
    
    // Write lines in REVERSE order (highest batch first, lowest last)
    for line in report_lines.iter().rev() {
        writeln!(report_file, "{}", line)?;
    }
    
    // Write summary at the end
    writeln!(report_file, "#")?;
    writeln!(report_file, "# Total files: {}", all_file_info.len())?;
    writeln!(report_file, "# Total lists: {}", cumulative.separated_string())?;
    
    debug_print(&format!("\n   Summary written to: {}", report_filename));
    debug_print(&format!("   Total files: {}", all_file_info.len()));
    debug_print(&format!("   Total lists: {}", cumulative.separated_string()));
    
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
        
        test_print(&format!("   Batch range: {:05} to {:05}", min_batch, max_batch));
        
        if missing_batches.is_empty() {
            test_print("   [OK] No missing batches in sequence");
        } else {
            test_print(&format!("   [!!] Found {} missing batches:", missing_batches.len()));
            for batch in &missing_batches {
                test_print(&format!("        - Batch {:05}", batch));
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
    let consolidated_count_file = format!("{}/no_set_list_count_{:02}.txt", base_path, target_size);
    let consolidated_path = std::path::Path::new(&consolidated_count_file);
    
    if consolidated_path.exists() {
        test_print(&format!("\n   Checking consolidated count file: no_set_list_count_{:02}.txt", target_size));
        
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
        test_print(&format!("\n   Consolidated count file not found: no_set_list_count_{:02}.txt", target_size));
        test_print("   Run --count mode first to generate count file");
    }
    
    // Step 3: Read intermediary count files and check for missing files
    let count_pattern = format!("no_set_list_intermediate_count_{:02}_", target_size);
    let entries = fs::read_dir(base_path)?;
    
    let mut intermediary_files: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with(&count_pattern) && name.ends_with(".txt") {
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
    use std::collections::BTreeMap;
    
    test_print(&format!("\nCompacting files for size {:02}...", target_size));
    test_print(&format!("Target batch size: {} lists per file", batch_size.separated_string()));
    
    let start_time = std::time::Instant::now();
    
    // Find all files for this target size
    let pattern = format!("*_to_{:02}_batch_*.rkyv", target_size);
    let paths = fs::read_dir(input_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                // Skip already compacted files
                !name_str.contains("compacted") && 
                wildmatch::WildMatch::new(&pattern).matches(&name_str)
            } else {
                false
            }
        })
        .collect::<Vec<_>>();
    
    if paths.is_empty() {
        test_print("   No files found to compact");
        return Ok(());
    }
    
    test_print(&format!("   Found {} files to compact", paths.len()));
    
    // Parse filenames and sort by target batch number
    let mut file_map: BTreeMap<u32, (u32, String)> = BTreeMap::new(); // target_batch -> (source_batch, filename)
    
    for path in &paths {
        if let Some(filename) = path.file_name() {
            let name = filename.to_string_lossy();
            // Parse: nsl_{src:02}_batch_{src_batch:05}_to_{tgt:02}_batch_{tgt_batch:05}.rkyv
            let parts: Vec<&str> = name.split('_').collect();
            if parts.len() >= 8 {
                if let (Ok(src_batch), Ok(tgt_batch)) = (
                    parts[3].parse::<u32>(),
                    parts[7].trim_end_matches(".rkyv").parse::<u32>()
                ) {
                    file_map.insert(tgt_batch, (src_batch, name.to_string()));
                }
            }
        }
    }
    
    // Load all lists from all files
    let mut all_lists: Vec<(NoSetListSerialized, u32)> = Vec::new(); // (list, source_batch)
    let mut total_loaded = 0u64;
    
    for (tgt_batch, (src_batch, filename)) in &file_map {
        let filepath = format!("{}/{}", input_dir, filename);
        test_print(&format!("   Loading {} (source batch {:05}, target batch {:05})...", 
            filename, src_batch, tgt_batch));
        
        match load_lists_from_file(&filepath) {
            Ok(lists) => {
                let count = lists.len();
                total_loaded += count as u64;
                for list in lists {
                    all_lists.push((list, *src_batch));
                }
                test_print(&format!("      Loaded {} lists", count.separated_string()));
            }
            Err(e) => {
                eprintln!("   Error loading {}: {}", filename, e);
                return Err(e);
            }
        }
    }
    
    test_print(&format!("\n   Total lists loaded: {}", total_loaded.separated_string()));
    
    // Create compacted files
    let mut new_batch = 0u32;
    let mut new_file_lists: Vec<NoSetListSerialized> = Vec::new();
    let mut first_source_batch: Option<u32> = None;
    let mut files_created = 0usize;
    
    for (list, src_batch) in all_lists {
        if first_source_batch.is_none() {
            first_source_batch = Some(src_batch);
        }
        
        new_file_lists.push(list);
        
        if new_file_lists.len() as u64 >= batch_size {
            // Save compacted file
            let filename = format!("{}/nsl_compacted_{:02}_batch_{:05}_from_{:05}.rkyv",
                output_dir, target_size, new_batch, first_source_batch.unwrap());
            
            test_print(&format!("   Creating compacted batch {:05} ({} lists, from source batch {:05})...",
                new_batch, new_file_lists.len().separated_string(), first_source_batch.unwrap()));
            
            save_compacted_batch(&filename, &new_file_lists)?;
            
            new_batch += 1;
            files_created += 1;
            new_file_lists.clear();
            first_source_batch = None;
        }
    }
    
    // Save remaining lists
    if !new_file_lists.is_empty() {
        let filename = format!("{}/nsl_compacted_{:02}_batch_{:05}_from_{:05}.rkyv",
            output_dir, target_size, new_batch, first_source_batch.unwrap());
        
        test_print(&format!("   Creating final compacted batch {:05} ({} lists, from source batch {:05})...",
            new_batch, new_file_lists.len().separated_string(), first_source_batch.unwrap()));
        
        save_compacted_batch(&filename, &new_file_lists)?;
        files_created += 1;
    }
    
    // Delete original files
    test_print("\n   Deleting original files...");
    for path in &paths {
        if let Err(e) = fs::remove_file(path) {
            eprintln!("   Warning: Could not delete {:?}: {}", path, e);
        }
    }
    
    let elapsed = start_time.elapsed().as_secs_f64();
    test_print(&format!("\nCompaction completed in {:.2} seconds", elapsed));
    test_print(&format!("   Original files: {}", paths.len()));
    test_print(&format!("   Compacted files: {}", files_created));
    test_print(&format!("   Total lists: {}", total_loaded.separated_string()));
    test_print(&format!("   Compression ratio: {:.1}x", paths.len() as f64 / files_created as f64));
    
    Ok(())
}

/// Load lists from a file (helper for compact mode)
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
    
    debug_print(&format!("get_next_output_batch_from_files: next batch for size {:02} = {:05} (scanned filenames only)",
        target_size, next_batch));
    
    next_batch
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
/// Uses 5-digit batch numbers for scalability
fn output_filename(
    base_path: &str,
    source_size: u8,
    source_batch: u32,
    target_size: u8,
    target_batch: u32
) -> String {
    use std::path::Path;
    let filename = format!(
        "nsl_{:02}_batch_{:05}_to_{:02}_batch_{:05}.rkyv",
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
    target_batch: u32
) -> Option<String> {
    use std::fs;
    
    let pattern = format!("_to_{:02}_batch_{:05}.rkyv", target_size, target_batch);
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
