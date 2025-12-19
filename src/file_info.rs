//! File state tracking and persistence module
//!
//! This module provides GlobalFileState for in-memory tracking of all output files
//! with atomic JSON/TXT persistence. It enables O(1) file lookups and incremental
//! state updates during processing.
//!
//! Key features:
//! - BTreeMap-backed in-memory state for fast lookups
//! - Multi-source loading: JSON (fast) → TXT → intermediary → rkyv scan
//! - Atomic persistence with .tmp files and rename
//! - File integrity checking and metadata tracking
//!
//! Used by all processing modes for state management

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::BufRead;
use separator::Separatable;
use std::path::{Path, PathBuf};

use memmap2::Mmap;
use rkyv::check_archived_root;
use rkyv::{Archive, Serialize as RkyvSerialize, Deserialize as RkyvDeserialize};
use serde::{Deserialize, Serialize};

use crate::no_set_list::NoSetListSerialized;
use crate::utils::debug_print;

/// Represents a single entry from the global count file plus on-disk metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct FileInfo {
    pub source_batch: u32,
    pub target_batch: u32,
    pub cumulative_nb_lists: u64,
    pub nb_lists_in_file: u64,
    pub filename: String,
    pub compacted: bool,
    // Optional runtime metadata gathered during checks
    pub exists: Option<bool>,
    pub file_size_bytes: Option<u64>,
    pub modified_timestamp: Option<i64>, // unix seconds
}

impl FileInfo {
    pub fn path_in(&self, base_dir: &str) -> PathBuf {
        Path::new(base_dir).join(&self.filename)
    }

    /// Update status fields by inspecting the file on disk. Optionally deep-count lists.
    pub fn refresh_status(&mut self, base_dir: &str, deep_check: bool) -> FileCheckResult {
        let path = self.path_in(base_dir);
        let mut result = FileCheckResult::for_file(&self.filename);

        match fs::metadata(&path) {
            Ok(meta) => {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);
                self.exists = Some(true);
                self.file_size_bytes = Some(meta.len());
                self.modified_timestamp = modified;
                result.exists = true;
                result.file_size_bytes = Some(meta.len());
                result.modified_timestamp = modified;
            }
            Err(e) => {
                self.exists = Some(false);
                result.error = Some(format!("metadata error: {}", e));
                return result;
            }
        }

        if deep_check {
            match count_lists_in_file(&path) {
                Ok(count) => {
                    self.nb_lists_in_file = count;
                    result.list_count = Some(count);
                }
                Err(e) => {
                    result.error = Some(format!("count error: {}", e));
                }
            }
        }

        result
    }
}

/// Aggregated file info list with helpers for JSON persistence and status checks.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
pub struct GlobalFileInfo {
    pub entries: Vec<FileInfo>,
}

impl GlobalFileInfo {
    pub fn new(entries: Vec<FileInfo>) -> Self {
        Self { entries }
    }

    pub fn save_json<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        Self::backup_if_exists(path.as_ref(), "json")?;
        let file = fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    pub fn load_json<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = fs::File::open(path)?;
        serde_json::from_reader(file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    /// Save to rkyv binary format (much faster than JSON)
    pub fn save_rkyv<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        use std::io::Write;
        Self::backup_if_exists(path.as_ref(), "rkyv")?;
        let bytes = rkyv::to_bytes::<_, 256>(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let mut file = fs::File::create(path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(())
    }

    /// Load from rkyv binary format
    pub fn load_rkyv<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = fs::File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let archived = check_archived_root::<Self>(&mmap[..])
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("rkyv validation error: {:?}", e)))?;
        let deserialized: Self = archived.deserialize(&mut rkyv::Infallible)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("rkyv deserialization error: {:?}", e)))?;
        Ok(deserialized)
    }

    /// Backup existing file by renaming to _old before saving new version
    fn backup_if_exists(path: &Path, extension: &str) -> std::io::Result<()> {
        if path.exists() {
            let old_path = path.with_extension(format!("{}_old", extension));
            if old_path.exists() {
                let _ = fs::remove_file(&old_path); // Remove previous backup
            }
            fs::rename(path, &old_path)?;
        }
        Ok(())
    }

    /// Load from a global count text file.
    pub fn from_global_count_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(Self { entries: parse_global_count_text(&text) })
    }

    /// Load from intermediary count files in a directory and build aggregated entries.
    /// This avoids reading all .rkyv files by trusting the intermediary counts.
    /// Idempotent: loads existing JSON first and only processes missing source batches.
    /// If force=true, ignores existing JSON and rebuilds from scratch.
    pub fn from_intermediary_files(base_path: &str, target_size: u8, force: bool) -> std::io::Result<Self> {
        use crate::utils::test_print;
        use separator::Separatable;
        
        let mut all_file_info: BTreeMap<(u32, u32), (String, u64, bool)> = BTreeMap::new();
        let mut seen_files: HashSet<String> = HashSet::new();
        let mut processed_source_batches: HashSet<u32> = HashSet::new();
        let pattern_new = format!("nsl_{:02}_intermediate_count_from_{:02}_", target_size, target_size - 1);
        let legacy_pattern = format!("no_set_list_input_intermediate_count_{:02}_", target_size - 1);
        
        // Step 1: Try to load existing file (unless force mode)
        // Try rkyv first (faster), fall back to JSON
        let json_path = Path::new(base_path).join(format!("nsl_{:02}_global_info.json", target_size));
        let rkyv_path_load = Path::new(base_path).join(format!("nsl_{:02}_global_info.rkyv", target_size));
        
        if !force {
            let mut loaded = false;
            
            // Try rkyv binary format first (much faster)
            if rkyv_path_load.exists() {
                test_print(&format!("   ... Loading existing rkyv file: {}", rkyv_path_load.display()));
                match Self::load_rkyv(&rkyv_path_load) {
                    Ok(existing_gfi) => {
                        // Extract existing data
                        for entry in existing_gfi.entries {
                            let key = (entry.source_batch, entry.target_batch);
                            all_file_info.insert(key, (entry.filename.clone(), entry.nb_lists_in_file, entry.compacted));
                            seen_files.insert(entry.filename.clone());
                            processed_source_batches.insert(entry.source_batch);
                        }
                        let unique_batches: HashSet<u32> = all_file_info.keys().map(|(src, _)| *src).collect();
                        test_print(&format!("   ... Loaded {} output files from {} input batches", 
                            all_file_info.len().separated_string(), unique_batches.len()));
                        loaded = true;
                    }
                    Err(e) => {
                        test_print(&format!("   ... Warning: Could not load rkyv file: {}", e));
                        test_print("   ... Trying JSON fallback...");
                    }
                }
            }
            
            // Fall back to JSON if rkyv failed or doesn't exist
            if !loaded && json_path.exists() {
                test_print(&format!("   ... Loading existing JSON file: {}", json_path.display()));
                match Self::load_json(&json_path) {
                    Ok(existing_gfi) => {
                        // Extract existing data
                        for entry in existing_gfi.entries {
                            let key = (entry.source_batch, entry.target_batch);
                            all_file_info.insert(key, (entry.filename.clone(), entry.nb_lists_in_file, entry.compacted));
                            seen_files.insert(entry.filename.clone());
                            processed_source_batches.insert(entry.source_batch);
                        }
                        let unique_batches: HashSet<u32> = all_file_info.keys().map(|(src, _)| *src).collect();
                        test_print(&format!("   ... Loaded {} output files from {} input batches", 
                            all_file_info.len().separated_string(), unique_batches.len()));
                    }
                    Err(e) => {
                        test_print(&format!("   ... Warning: Could not load existing JSON: {}", e));
                        test_print("   ... Will rebuild from scratch");
                    }
                }
            }
        }
        
        // Step 2: Collect all intermediary files and extract their source batch numbers
        let mut intermediary_files_with_batches: Vec<(std::path::PathBuf, u32)> = Vec::new();
        for entry in fs::read_dir(base_path)? {
            if let Ok(e) = entry {
                if let Some(name) = e.file_name().to_str() {
                    if (name.starts_with(&pattern_new) || name.starts_with(&legacy_pattern)) && name.ends_with(".txt") {
                        // Extract source batch number from filename
                        if let Some(batch_str) = name.rsplit('_').next().and_then(|s| s.strip_suffix(".txt")) {
                            if let Ok(batch) = batch_str.parse::<u32>() {
                                intermediary_files_with_batches.push((e.path(), batch));
                            }
                        }
                    }
                }
            }
        }
        
        if intermediary_files_with_batches.is_empty() {
            if all_file_info.is_empty() {
                test_print("   ... No intermediary count files found, scanning .rkyv files directly...");
                let scanned = scan_rkyv_files(base_path, target_size)?;
                return Ok(Self { entries: scanned });
            } else {
                // We have data from JSON, no new intermediary files to process
                test_print("   ... No new intermediary files to process, using existing JSON data");
                let mut entries: Vec<FileInfo> = all_file_info
                    .into_iter()
                    .map(|((src, tgt), (fname, count, compacted))| FileInfo {
                        source_batch: src,
                        target_batch: tgt,
                        cumulative_nb_lists: 0,
                        nb_lists_in_file: count,
                        filename: fname,
                        compacted,
                        exists: None,
                        file_size_bytes: None,
                        modified_timestamp: None,
                    })
                    .collect();
                entries.sort_by(|a, b| match a.target_batch.cmp(&b.target_batch) {
                    std::cmp::Ordering::Equal => a.source_batch.cmp(&b.source_batch),
                    other => other,
                });
                let mut cumulative = 0u64;
                for e in entries.iter_mut() {
                    cumulative += e.nb_lists_in_file;
                    e.cumulative_nb_lists = cumulative;
                }
                return Ok(Self { entries });
            }
        }
        
        // Step 3: Filter to only unprocessed batches
        let mut files_to_process: Vec<(std::path::PathBuf, u32)> = intermediary_files_with_batches
            .into_iter()
            .filter(|(_, batch)| !processed_source_batches.contains(batch))
            .collect();
        files_to_process.sort_by_key(|(_, batch)| *batch);
        
        let total_files = files_to_process.len();
        let already_processed = processed_source_batches.len();
        
        if total_files == 0 {
            test_print(&format!("   ... All {} input batches already processed in JSON", already_processed));
            // Return existing data
            let mut entries: Vec<FileInfo> = all_file_info
                .into_iter()
                .map(|((src, tgt), (fname, count, compacted))| FileInfo {
                    source_batch: src,
                    target_batch: tgt,
                    cumulative_nb_lists: 0,
                    nb_lists_in_file: count,
                    filename: fname,
                    compacted,
                    exists: None,
                    file_size_bytes: None,
                    modified_timestamp: None,
                })
                .collect();
            entries.sort_by(|a, b| match a.target_batch.cmp(&b.target_batch) {
                std::cmp::Ordering::Equal => a.source_batch.cmp(&b.source_batch),
                other => other,
            });
            let mut cumulative = 0u64;
            for e in entries.iter_mut() {
                cumulative += e.nb_lists_in_file;
                e.cumulative_nb_lists = cumulative;
            }
            return Ok(Self { entries });
        }
        
        test_print(&format!("   ... {} input batches already processed, {} new batches to process", 
            already_processed, total_files));
        test_print(&format!("   ... Reading {} new intermediary files and updating registry...", total_files));
        
        let save_interval = 100; // Save every 100 files
        let rkyv_path = Path::new(base_path).join(format!("nsl_{:02}_global_info.rkyv", target_size));
        
        for (idx, (path, source_batch)) in files_to_process.iter().enumerate() {
            let file_num = idx + 1;
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Show progress every file
                test_print(&format!("   ... [{:>4}/{:<4}] Reading: {} (input batch {:06})", file_num, total_files, name, source_batch));
                
                let file = fs::File::open(&path)?;
                let reader = std::io::BufReader::new(file);
                let mut lines_in_file = 0;
                for line in reader.lines() {
                    let line = line?;
                    if line.trim().starts_with("...") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 5 {
                            if let Ok(count) = parts[1].parse::<u64>() {
                                let filename = parts[4];
                                if seen_files.contains(filename) {
                                    continue;
                                }
                                let (src_batch, tgt_batch) = match parse_batches(filename) {
                                    Some(v) => v,
                                    None => continue,
                                };
                                let compacted = filename.contains("_compacted.rkyv");
                                seen_files.insert(filename.to_string());
                                all_file_info.insert((src_batch, tgt_batch), (filename.to_string(), count, compacted));
                                lines_in_file += 1;
                            }
                        }
                    }
                }
                test_print(&format!("             ... {} output files tracked in this intermediary", lines_in_file));
                
                // Save progress every save_interval files
                if file_num % save_interval == 0 || file_num == total_files {
                    let save_start = std::time::Instant::now();
                    test_print(&format!("   ... Saving intermediate progress ({} files processed, {} unique output files)...", 
                        file_num, all_file_info.len().separated_string()));
                    
                    // Build current entries and save to rkyv binary format (much faster than JSON)
                    let mut entries: Vec<FileInfo> = all_file_info
                        .iter()
                        .map(|((src, tgt), (fname, count, compacted))| FileInfo {
                            source_batch: *src,
                            target_batch: *tgt,
                            cumulative_nb_lists: 0,
                            nb_lists_in_file: *count,
                            filename: fname.clone(),
                            compacted: *compacted,
                            exists: None,
                            file_size_bytes: None,
                            modified_timestamp: None,
                        })
                        .collect();
                    
                    entries.sort_by(|a, b| match a.target_batch.cmp(&b.target_batch) {
                        std::cmp::Ordering::Equal => a.source_batch.cmp(&b.source_batch),
                        other => other,
                    });
                    
                    let mut cumulative = 0u64;
                    for e in entries.iter_mut() {
                        cumulative += e.nb_lists_in_file;
                        e.cumulative_nb_lists = cumulative;
                    }
                    
                    let temp_gfi = GlobalFileInfo { entries };
                    // Use rkyv binary format for intermediate saves (10-100x faster than JSON)
                    if let Err(e) = temp_gfi.save_rkyv(&rkyv_path) {
                        test_print(&format!("   ... Warning: Could not save intermediate progress: {}", e));
                    } else {
                        let elapsed = save_start.elapsed().as_secs_f64();
                        test_print(&format!("             ... Saved in {:.2}s", elapsed));
                    }
                }
            }
        }

        test_print(&format!("   ... Completed reading {} intermediary files, found {} unique output files", 
            total_files, all_file_info.len().separated_string()));
        
        let mut entries: Vec<FileInfo> = all_file_info
            .into_iter()
            .map(|((src, tgt), (fname, count, compacted))| FileInfo {
                source_batch: src,
                target_batch: tgt,
                cumulative_nb_lists: 0,
                nb_lists_in_file: count,
                filename: fname,
                compacted,
                exists: None,
                file_size_bytes: None,
                modified_timestamp: None,
            })
            .collect();

        // If no intermediary info was found (common for seeds/size 03), fall back to scanning .rkyv files directly.
        if entries.is_empty() {
            debug_print(&format!("   ... No intermediary files found, scanning .rkyv files directly..."));
            let scanned = scan_rkyv_files(base_path, target_size)?;
            return Ok(Self { entries: scanned });
        }

        entries.sort_by(|a, b| match a.target_batch.cmp(&b.target_batch) {
            std::cmp::Ordering::Equal => a.source_batch.cmp(&b.source_batch),
            other => other,
        });

        let mut cumulative = 0u64;
        for e in entries.iter_mut() {
            cumulative += e.nb_lists_in_file;
            e.cumulative_nb_lists = cumulative;
        }

        Ok(Self { entries })
    }

    /// Run status checks on all entries, optionally deep-counting list totals.
    pub fn check_all(&mut self, base_dir: &str, deep_check: bool) -> Vec<FileCheckResult> {
        self.entries
            .iter_mut()
            .map(|fi| fi.refresh_status(base_dir, deep_check))
            .collect()
    }

    /// Render to text in the same layout as legacy global count and write as nsl_{size}_global_info.txt.
    pub fn to_txt(&self, base_dir: &str, target_size: u8) -> String {
        render_global_count(&self.entries, target_size, base_dir)
    }
}




/// Mutable, incremental state for file info with atomic flush helpers.
#[derive(Debug, Clone)]
pub struct GlobalFileState {
    target_size: u8,
    base_dir: String,
    entries: BTreeMap<(u32, u32, String), FileInfo>,
}

impl GlobalFileState {
    fn key(src: u32, tgt: u32, filename: &str) -> (u32, u32, String) {
        (src, tgt, filename.to_string())
    }
    
    pub fn new(base_dir: &str, target_size: u8) -> Self {
        Self { target_size, base_dir: base_dir.to_string(), entries: BTreeMap::new() }
    }

    pub fn from_sources(base_dir: &str, target_size: u8) -> std::io::Result<Self> {
        // Priority 1: rkyv (authoritative format)
        let rkyv_path = Path::new(base_dir).join(format!("nsl_{:02}_global_info.rkyv", target_size));
        if rkyv_path.exists() {
            let gfi = GlobalFileInfo::load_rkyv(&rkyv_path)?;
            return Ok(Self::from_vec(base_dir, target_size, gfi.entries));
        }
        
        // Priority 2: JSON (legacy format, migration path)
        let json_path = Path::new(base_dir).join(format!("nsl_{:02}_global_info.json", target_size));
        if json_path.exists() {
            let gfi = GlobalFileInfo::load_json(&json_path)?;
            return Ok(Self::from_vec(base_dir, target_size, gfi.entries));
        }
        
        // Priority 3: Legacy global_count.txt files
        let primary = Path::new(base_dir).join(format!("nsl_{:02}_global_count.txt", target_size));
        let legacy_space = Path::new(base_dir).join(format!("nsl_{:02}_global count.txt", target_size));
        if primary.exists() {
            let gfi = GlobalFileInfo::from_global_count_file(&primary)?;
            return Ok(Self::from_vec(base_dir, target_size, gfi.entries));
        } else if legacy_space.exists() {
            let gfi = GlobalFileInfo::from_global_count_file(&legacy_space)?;
            return Ok(Self::from_vec(base_dir, target_size, gfi.entries));
        }
        
        // Priority 4: Legacy intermediate count files (slowest)
        let gfi = GlobalFileInfo::from_intermediary_files(base_dir, target_size, false)?;
        Ok(Self::from_vec(base_dir, target_size, gfi.entries))
    }

    fn from_vec(base_dir: &str, target_size: u8, entries: Vec<FileInfo>) -> Self {
        let mut map = BTreeMap::new();
        for e in entries {
            map.insert(Self::key(e.source_batch, e.target_batch, &e.filename), e);
        }
        let mut state = Self { target_size, base_dir: base_dir.to_string(), entries: map };
        state.recompute_cumulative();
        state
    }

    pub fn register_file(
        &mut self,
        filename: &str,
        src_batch: u32,
        tgt_batch: u32,
        nb_lists_in_file: u64,
        compacted: bool,
        file_size_bytes: Option<u64>,
        modified_timestamp: Option<i64>,
    ) {
        let fi = FileInfo {
            source_batch: src_batch,
            target_batch: tgt_batch,
            cumulative_nb_lists: 0,
            nb_lists_in_file,
            filename: filename.to_string(),
            compacted,
            exists: Some(true),
            file_size_bytes,
            modified_timestamp,
        };
        self.entries.insert(Self::key(src_batch, tgt_batch, filename), fi);
        self.recompute_cumulative();
    }

    pub fn remove_file(&mut self, filename: &str, src_batch: u32, tgt_batch: u32) {
        self.entries.remove(&Self::key(src_batch, tgt_batch, filename));
        self.recompute_cumulative();
    }

    pub fn update_count(&mut self, filename: &str, src_batch: u32, tgt_batch: u32, nb_lists_in_file: u64) {
        if let Some(e) = self.entries.get_mut(&Self::key(src_batch, tgt_batch, filename)) {
            e.nb_lists_in_file = nb_lists_in_file;
            e.cumulative_nb_lists = 0;
            self.recompute_cumulative();
        }
    }

    pub fn entries(&self) -> &BTreeMap<(u32, u32, String), FileInfo> {
        &self.entries
    }

    pub fn to_vec(&self) -> Vec<FileInfo> {
        let mut v: Vec<FileInfo> = self.entries.values().cloned().collect();
        v.sort_by(|a, b| match a.target_batch.cmp(&b.target_batch) {
            std::cmp::Ordering::Equal => match a.source_batch.cmp(&b.source_batch) {
                std::cmp::Ordering::Equal => a.filename.cmp(&b.filename),
                other => other,
            },
            other => other,
        });
        v
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.recompute_cumulative();
        let entries_vec = self.to_vec();
        let gfi = GlobalFileInfo { entries: entries_vec };

        // Save to rkyv as authoritative format
        let rkyv_path = Path::new(&self.base_dir).join(format!("nsl_{:02}_global_info.rkyv", self.target_size));
        
        // Backup existing rkyv file before overwriting
        if rkyv_path.exists() {
            let backup_path = rkyv_path.with_extension("rkyv.old");
            let _ = fs::rename(&rkyv_path, &backup_path);
        }
        
        // Write to temp file, then rename atomically
        let rkyv_tmp = rkyv_path.with_extension("rkyv.tmp");
        gfi.save_rkyv(&rkyv_tmp)?;
        fs::rename(rkyv_tmp, &rkyv_path)?;

        Ok(())
    }
    
    /// Export human-readable JSON and TXT files from the current state
    /// This is a write-only operation - these files are not read during normal operation
    pub fn export_human_readable(&self) -> std::io::Result<()> {
        let entries_vec = self.to_vec();
        let gfi = GlobalFileInfo { entries: entries_vec.clone() };

        let json_path = Path::new(&self.base_dir).join(format!("nsl_{:02}_global_info.json", self.target_size));
        let txt_path = Path::new(&self.base_dir).join(format!("nsl_{:02}_global_info.txt", self.target_size));

        // JSON export
        let json_tmp = json_path.with_extension("json.tmp");
        let json_text = serde_json::to_string_pretty(&gfi)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(&json_tmp, json_text)?;
        if json_path.exists() { let _ = fs::remove_file(&json_path); }
        fs::rename(json_tmp, &json_path)?;

        // TXT export
        let txt_tmp = txt_path.with_extension("txt.tmp");
        let txt_body = render_global_count(&entries_vec, self.target_size, &self.base_dir);
        fs::write(&txt_tmp, txt_body)?;
        if txt_path.exists() { let _ = fs::remove_file(&txt_path); }
        fs::rename(txt_tmp, &txt_path)?;

        Ok(())
    }

    fn recompute_cumulative(&mut self) {
        let mut entries_sorted: Vec<_> = self.entries.values_mut().collect();
        entries_sorted.sort_by(|a, b| match a.target_batch.cmp(&b.target_batch) {
            std::cmp::Ordering::Equal => match a.source_batch.cmp(&b.source_batch) {
                std::cmp::Ordering::Equal => a.filename.cmp(&b.filename),
                other => other,
            },
            other => other,
        });
        let mut cumulative = 0u64;
        for e in entries_sorted {
            cumulative += e.nb_lists_in_file;
            e.cumulative_nb_lists = cumulative;
        }
    }
}
/// Result of checking one file on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileCheckResult {
    pub filename: String,
    pub exists: bool,
    pub file_size_bytes: Option<u64>,
    pub modified_timestamp: Option<i64>,
    pub list_count: Option<u64>,
    pub error: Option<String>,
}

impl FileCheckResult {
    pub fn for_file(filename: &str) -> Self {
        Self {
            filename: filename.to_string(),
            exists: false,
            file_size_bytes: None,
            modified_timestamp: None,
            list_count: None,
            error: None,
        }
    }
}

/// Count lists quickly without deserializing fully.
fn count_lists_in_file(path: &Path) -> std::io::Result<u64> {
    let file = fs::File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    match check_archived_root::<Vec<NoSetListSerialized>>(&mmap[..]) {
        Ok(arch) => Ok(arch.len() as u64),
        Err(e) => {
            debug_print(&format!("   ... validation failed for {}: {:?}", path.display(), e));
            Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Archive validation failed"))
        }
    }
}

/// Utility to derive FileInfo rows from the existing global count text.
pub fn parse_global_count_text(text: &str) -> Vec<FileInfo> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split('|').collect();
        if parts.len() < 4 {
            continue;
        }
        let src_tgt = parts[0].trim();
        let fields: Vec<&str> = src_tgt.split_whitespace().collect();
        if fields.len() < 2 {
            continue;
        }
        let (Ok(src), Ok(tgt)) = (fields[0].parse::<u32>(), fields[1].parse::<u32>()) else {
            continue;
        };
        let cumulative = parts.get(1).and_then(|s| parse_num(s.trim())).unwrap_or(0);
        let nb_lists = parts.get(2).and_then(|s| parse_num(s.trim())).unwrap_or(0);
        let filename = parts.get(3).map(|s| s.trim().to_string()).unwrap_or_default();
        let compacted = parts.get(4).map(|s| s.trim().eq_ignore_ascii_case("compacted")).unwrap_or(false);

        entries.push(FileInfo {
            source_batch: src,
            target_batch: tgt,
            cumulative_nb_lists: cumulative,
            nb_lists_in_file: nb_lists,
            filename,
            compacted,
            exists: None,
            file_size_bytes: None,
            modified_timestamp: None,
        });
    }
    entries
}

fn parse_num(field: &str) -> Option<u64> {
    let digits: String = field.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.parse::<u64>().ok()
}

/// Build a pretty text report (global-count style) from FileInfo entries.
pub fn render_global_count(entries: &[FileInfo], target_size: u8, base_path: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("# File Count Summary for no-set-{:02} lists", target_size));
    lines.push(format!("# Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
    lines.push(format!("# Input directory: {}", base_path));
    lines.push(format!("# Intermediary files used: N/A"));
    lines.push("# Format: source_batch target_batch | cumulative_nb_lists | nb_lists_in_file | filename | compacted".to_string());
    lines.push("#".to_string());

    let mut cumulative = 0u64;
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| match a.target_batch.cmp(&b.target_batch) {
        std::cmp::Ordering::Equal => a.source_batch.cmp(&b.source_batch),
        other => other,
    });

    for e in &mut sorted {
        if e.cumulative_nb_lists == 0 {
            cumulative += e.nb_lists_in_file;
            e.cumulative_nb_lists = cumulative;
        } else {
            cumulative = e.cumulative_nb_lists;
        }
        lines.push(format!(
            "{:06} {:06} | {:>17} | {:>17} | {} | {}",
            e.source_batch,
            e.target_batch,
            e.cumulative_nb_lists.separated_string(),
            e.nb_lists_in_file.separated_string(),
            e.filename,
            if e.compacted { "compacted" } else { "" }
        ));
    }

    lines.push("#".to_string());
    lines.push(format!("# Total files: {}", sorted.len()));
    lines.push(format!("# Total lists: {}", cumulative.separated_string()));
    lines.join("\n")
}

/// Build FileInfo rows directly from disk (.rkyv files) without intermediaries.
pub fn scan_rkyv_files(base_path: &str, target_size: u8) -> std::io::Result<Vec<FileInfo>> {
    let mut entries: Vec<FileInfo> = Vec::new();
    let pattern = format!("_to_{:02}_batch_", target_size);
    for entry in fs::read_dir(base_path)? {
        if let Ok(e) = entry {
            if let Some(name) = e.file_name().to_str() {
                if name.starts_with("nsl_") && name.contains(&pattern) && name.ends_with(".rkyv") {
                    let filename = name.to_string();
                    let compacted = name.contains("_compacted.rkyv");
                    let (src_batch, tgt_batch) = parse_batches(&filename).unwrap_or((0, 0));
                    let count = count_lists_in_file(&e.path()).unwrap_or(0);
                    entries.push(FileInfo {
                        source_batch: src_batch,
                        target_batch: tgt_batch,
                        cumulative_nb_lists: 0,
                        nb_lists_in_file: count,
                        filename,
                        compacted,
                        exists: Some(true),
                        file_size_bytes: e.metadata().ok().map(|m| m.len()),
                        modified_timestamp: e
                            .metadata()
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64),
                    });
                }
            }
        }
    }
    entries.sort_by(|a, b| match a.target_batch.cmp(&b.target_batch) {
        std::cmp::Ordering::Equal => a.source_batch.cmp(&b.source_batch),
        other => other,
    });
    // Fill cumulative
    let mut cumulative = 0u64;
    for e in entries.iter_mut() {
        cumulative += e.nb_lists_in_file;
        e.cumulative_nb_lists = cumulative;
    }
    Ok(entries)
}

fn parse_batches(filename: &str) -> Option<(u32, u32)> {
    if let Some(to_pos) = filename.find("_to_") {
        let before_to = &filename[..to_pos];
        let after_to = &filename[to_pos + 4..];
        if let Some(src_batch_pos) = before_to.rfind("_batch_") {
            let src_str = &before_to[src_batch_pos + 7..];
            if let Some(tgt_batch_pos) = after_to.rfind("_batch_") {
                let tgt_str = &after_to[tgt_batch_pos + 7..after_to.len() - 5];
                if let (Ok(src), Ok(tgt)) = (src_str.parse::<u32>(), tgt_str.parse::<u32>()) {
                    return Some((src, tgt));
                }
            }
        }
    }
    None
}
