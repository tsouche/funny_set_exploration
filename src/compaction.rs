use std::fs;
use std::path::Path;
use memmap2::Mmap;
use rkyv::check_archived_root;
use rkyv::ser::{serializers::AllocSerializer, Serializer};
use rkyv::Deserialize;
use separator::Separatable;

use crate::no_set_list::NoSetListSerialized;
use crate::filenames;
use crate::utils::*;

/// Save compacted batch atomically: serialize to temp file, fsync, rename into place.
fn save_compacted_batch_atomic(filepath: &str, lists: &[NoSetListSerialized]) -> std::io::Result<()> {
    // Serialize
    let lists_vec: Vec<NoSetListSerialized> = lists.to_vec();
    let mut serializer = AllocSerializer::<4096>::default();
    serializer.serialize_value(&lists_vec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Serialization error: {:?}", e)))?;
    let bytes = serializer.into_serializer().into_inner();

    let path = Path::new(filepath);
    // Add PID suffix to tmp extension to avoid collisions
    let pid = std::process::id();
    let tmp = path.with_extension(format!("tmp.{}", pid));
    // Write tmp
    std::fs::write(&tmp, &bytes)?;
    // fsync
    let f = std::fs::File::open(&tmp)?;
    f.sync_all()?;
    // Remove existing target (Windows requires remove before rename)
    if path.exists() { let _ = std::fs::remove_file(path); }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Compact small output files into larger batches (idempotent and safe)
pub fn compact_size_files(input_dir: &str, output_dir: &str, target_size: u8, batch_size: u64) -> std::io::Result<()> {
    use std::io::Write;
    test_print(&format!("\nCompacting files for size {:02}...", target_size));
    test_print(&format!("Target batch size: {} lists per file", batch_size.separated_string()));

    let start_time = std::time::Instant::now();

    // Enforce in-place compaction: input_dir and output_dir must be identical
    if input_dir != output_dir {
        test_print(&format!("Compaction must run in-place. Use the same directory for input and output: {}", input_dir));
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Compaction requires in-place input/output (use same directory)"));
    }

    // Build plan using existing global count if present, otherwise intermediary files
    let report_filename = format!("{}/nsl_{:02}_global_count.txt", input_dir, target_size);
    let mut file_counts: Vec<(String, u64, u32, u32)> = Vec::new();
    let mut total_lists = 0u64;

    if Path::new(&report_filename).exists() {
        test_print(&format!("   Found consolidated count file: {}", report_filename));
        use std::io::{BufRead, BufReader};
        let file = fs::File::open(&report_filename)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() || line.trim().starts_with('#') { continue; }
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 4 {
                let src_tgt = parts[0].trim();
                let fields: Vec<&str> = src_tgt.split_whitespace().collect();
                if fields.len() >= 2 {
                    if let (Ok(src), Ok(tgt)) = (fields[0].parse::<u32>(), fields[1].parse::<u32>()) {
                        let count_str = parts[2].trim();
                        let digits_only: String = count_str.chars().filter(|c| c.is_ascii_digit()).collect();
                        if let Ok(count) = digits_only.parse::<u64>() {
                            let filename = parts[3].trim().to_string();
                            // Compact flag is optional (backwards compatible). If present use parts[4].
                            let mut is_compacted = filename.contains("_compacted.rkyv");
                            if parts.len() >= 5 {
                                let flag = parts[4].trim().to_lowercase();
                                if flag == "true" || flag == "yes" || flag == "1" {
                                    is_compacted = true;
                                } else if flag == "false" || flag == "no" || flag == "0" {
                                    is_compacted = false;
                                }
                            }
                            // Only include non-compacted files in the compaction plan
                            if !is_compacted {
                                file_counts.push((filename, count, src, tgt));
                                total_lists += count;
                            }
                        }
                    }
                }
            }
        }
    }

    // Debug: print plan order as parsed (src,tgt)
    debug_print(&format!("compaction: parsed {} files, order: {:?}", file_counts.len(), file_counts.iter().map(|f| (f.2, f.3)).collect::<Vec<_>>()));

    // If no consolidated counts, fallback to intermediary input files
    if file_counts.is_empty() {
        let prev_size = target_size - 1;
        let pattern = format!("no_set_list_input_intermediate_count_{:02}_", prev_size);
        let mut intermediary_files: Vec<String> = Vec::new();
        for entry in fs::read_dir(input_dir)? {
            if let Ok(entry) = entry {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&pattern) && name.ends_with(".txt") {
                        intermediary_files.push(format!("{}/{}", input_dir, name));
                    }
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
            file_counts.sort_by(|a, b| match a.3.cmp(&b.3) { std::cmp::Ordering::Equal => a.2.cmp(&b.2), other => other });
        }
    }

    // Ensure plan is ordered by target_batch then source_batch (ascending)
    file_counts.sort_by(|a, b| match a.3.cmp(&b.3) { std::cmp::Ordering::Equal => a.2.cmp(&b.2), other => other });

    if file_counts.is_empty() {
        test_print("   No consolidated or intermediary count files found. Run --count <SIZE> first to generate counts.");
        return Ok(());
    }

    test_print(&format!("\n   Total lists accounted for in plan: {}", total_lists.separated_string()));

    const READ_CHUNK_SIZE: usize = 2_000_000;

    if !Path::new(output_dir).exists() { fs::create_dir_all(output_dir)?; }

    let expected_compacted_files = ((total_lists + batch_size - 1) / batch_size) as usize;
    test_print(&format!("\n   Plan: Create ~{} compacted files (batch size {}) from {} input files", expected_compacted_files, batch_size.separated_string(), file_counts.len()));

    let source_size = target_size - 1;

    // Determine starting compact batch by scanning existing compacted files only.
    // Start at 0 if none found so compacted outputs are numbered from 000000 upwards.
    let mut current_compact_batch: u32 = 0;
    if let Ok(entries) = std::fs::read_dir(output_dir) {
        let mut max_compacted: Option<u32> = None;
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with("_compacted.rkyv") && name.contains(&format!("_to_{:02}_batch_", target_size)) {
                    if let Some(to_pos) = name.find("_to_") {
                        // extract after 'to_'
                        let after_to = &name[to_pos + 4..];
                        if let Some(batch_pos) = after_to.rfind("_batch_") {
                            // batch number runs until "_compacted.rkyv"
                            let start = batch_pos + 7;
                            let end = after_to.len() - "_compacted.rkyv".len();
                            if end > start && end <= after_to.len() {
                                let batch_str = &after_to[start..end];
                                if let Ok(num) = batch_str.parse::<u32>() {
                                    max_compacted = Some(max_compacted.map_or(num, |m| m.max(num)));
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(m) = max_compacted { current_compact_batch = m + 1; }
    }
    let mut current_output_buffer: Vec<NoSetListSerialized> = Vec::new();
    let mut current_last_source: Option<u32> = None;
    let mut current_output_contribs: Vec<(u32, u64)> = Vec::new();
    let mut files_created = 0usize;

    // Helper to flush buffer into a compacted file (safely and idempotently)
    let flush_current = |output_dir: &str,
                         target_size: u8,
                         batch_idx: u32,
                         last_source_batch: Option<u32>,
                         buffer: &mut Vec<NoSetListSerialized>,
                         contribs: &[(u32, u64)],
                         input_dir: &str,
                         source_size: u8|
                         -> std::io::Result<()> {
        if buffer.is_empty() { return Ok(()); }
        // Determine new input batch as the LAST contributing source batch per spec
        let from_src = last_source_batch.unwrap_or_else(|| if !contribs.is_empty() { contribs.last().unwrap().0 } else { 0 });
        // If buffer is a full batch, mark as compacted; otherwise final partial file remains non-compacted
        let is_full = (buffer.len() as u64) >= batch_size;
        let filename = if is_full {
            format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}_compacted.rkyv", output_dir, source_size, from_src, target_size, batch_idx)
        } else {
            format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", output_dir, source_size, from_src, target_size, batch_idx)
        };

        // If target file already exists, assume it's correct and skip (idempotent)
        if Path::new(&filename).exists() {
            test_print(&format!("   Skipping existing compacted file {}", filename));
        } else {
            test_print(&format!("   Writing compacted file {} ({} lists)", filename, buffer.len().separated_string()));
            save_compacted_batch_atomic(&filename, buffer)?;
        }

        // Append intermediary count files to input directory for provenance (idempotent append)
        use std::fs::OpenOptions;
        for (src_batch, cnt) in contribs {
            let inter_filename = format!("{}/nsl_{:02}_intermediate_count_from_{:02}_{:06}.txt", input_dir, target_size, source_size, src_batch);
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&inter_filename) {
                if let Err(e) = writeln!(f, "   ... {:>8} lists in {}", cnt, Path::new(&filename).file_name().unwrap().to_string_lossy()) {
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
    for (base_name, _count, src_batch, _tgt_batch) in &file_counts {
        let filepath = format!("{}/{}", input_dir, base_name);
        test_print(&format!("\n   Processing input file {}", base_name));

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
            let mut chunk: Vec<NoSetListSerialized> = Vec::with_capacity(take);
            for i in idx..(idx + take) {
                let archived_elem = archived.get(i).expect("index in range");
                let des: NoSetListSerialized = archived_elem.deserialize(&mut rkyv::Infallible).expect("deserialization");
                chunk.push(des);
            }
            idx += take;

            let mut chunk_idx = 0usize;
            while chunk_idx < chunk.len() {
                let space_left = (batch_size as usize) - current_output_buffer.len();
                let take_now = std::cmp::min(space_left, chunk.len() - chunk_idx);

                if current_last_source.is_none() { current_last_source = Some(*src_batch); } else { current_last_source = Some(*src_batch); }
                let mut found = false;
                for entry in current_output_contribs.iter_mut() {
                    if entry.0 == *src_batch { entry.1 += take_now as u64; found = true; break; }
                }
                if !found { current_output_contribs.push((*src_batch, take_now as u64)); }

                current_output_buffer.extend(chunk[chunk_idx..chunk_idx + take_now].iter().cloned());
                chunk_idx += take_now;

                if current_output_buffer.len() as u64 >= batch_size {
                    flush_current(output_dir, target_size, current_compact_batch, current_last_source, &mut current_output_buffer, &current_output_contribs, input_dir, source_size)?;
                    files_created += 1;
                    current_compact_batch += 1;
                    current_last_source = None;
                    current_output_contribs.clear();
                }
            }
        }
    }

    if !current_output_buffer.is_empty() {
        // For final partial file we pass last source batch so naming uses last contributor
        flush_current(output_dir, target_size, current_compact_batch, current_last_source, &mut current_output_buffer, &current_output_contribs, input_dir, source_size)?;
        files_created += 1;
        current_output_contribs.clear();
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    test_print(&format!("\nCompaction completed in {:.2} seconds", elapsed));
    test_print(&format!("   Input files: {}", file_counts.len()));
    test_print(&format!("   Compacted files: {}", files_created));
    test_print(&format!("   Total lists: {}", total_lists.separated_string()));

    Ok(())
}

/// Compact a single non-compacted input file into one compacted output file in-place.
/// Behavior:
/// - Finds the first non-compacted file with the lowest output-batch number.
/// - Determines next compacted output index from existing `_compacted.rkyv` files (or 0).
/// - Reads up to `batch_size` lists from that file, writes a compacted file (atomic).
/// - Removes those lists from the origin file: deletes origin if empty, otherwise rewrites it
///   with the remaining lists (atomic).
pub fn compact_one_file_inplace(dir: &str, target_size: u8, batch_size: u64) -> std::io::Result<()> {
    use std::io::Write;
    test_print(&format!("\nSingle-file compaction for size {:02} (batch_size={})", target_size, batch_size.separated_string()));

    // Find non-compacted input files and sort ascending by target_batch then source_batch
    let mut candidates: Vec<(String, u32, u32)> = Vec::new(); // (filename, src_batch, tgt_batch)
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => return Err(e),
    };
    let pattern = format!("_to_{:02}_batch_", target_size);
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with("nsl_") && name.contains(&pattern) && !name.contains("_compacted.rkyv") && name.ends_with(".rkyv") {
                if let Some(to_pos) = name.find("_to_") {
                    let before_to = &name[..to_pos];
                    let after_to = &name[to_pos + 4..];
                    if let Some(src_batch_pos) = before_to.rfind("_batch_") {
                        let src_batch_str = &before_to[src_batch_pos + 7..];
                        if let Ok(srcb) = src_batch_str.parse::<u32>() {
                            if let Some(tgt_batch_pos) = after_to.rfind("_batch_") {
                                let tgt_batch_str = &after_to[tgt_batch_pos + 7..after_to.len() - 5];
                                if let Ok(tgtb) = tgt_batch_str.parse::<u32>() {
                                    candidates.push((name.to_string(), srcb, tgtb));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if candidates.is_empty() {
        test_print("   No non-compacted files found to process");
        return Ok(());
    }

    candidates.sort_by(|a, b| match a.2.cmp(&b.2) { std::cmp::Ordering::Equal => a.1.cmp(&b.1), other => other });
    let (first_name, first_src, _first_tgt) = candidates[0].clone();
    test_print(&format!("   Will read from first non-compacted file: {} (src={:06})", first_name, first_src));

    // Determine next compacted batch index from existing _compacted files (start at 0 if none)
    let mut next_compacted_idx: u32 = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut max_idx: Option<u32> = None;
        for entry in entries.flatten() {
            if let Some(n) = entry.file_name().to_str() {
                if n.ends_with("_compacted.rkyv") && n.contains(&pattern) {
                    if let Some(to_pos) = n.find("_to_") {
                        let after_to = &n[to_pos + 4..];
                        if let Some(batch_pos) = after_to.rfind("_batch_") {
                            let start = batch_pos + 7;
                            let end = after_to.len() - "_compacted.rkyv".len();
                            if end > start && end <= after_to.len() {
                                let batch_str = &after_to[start..end];
                                if let Ok(num) = batch_str.parse::<u32>() {
                                    max_idx = Some(max_idx.map_or(num, |m| m.max(num)));
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(m) = max_idx { next_compacted_idx = m + 1; }
    }
    test_print(&format!("   Next compacted index = {:06}", next_compacted_idx));

    // Load lists from first file
    let filepath = format!("{}/{}", dir, first_name);
    let file = std::fs::File::open(&filepath)?;
    let mmap = unsafe { Mmap::map(&file)? };
    let archived = check_archived_root::<Vec<NoSetListSerialized>>(&mmap[..])
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Archive validation failed"))?;

    let total = archived.len();
    test_print(&format!("   Source file contains {} lists", total.separated_string()));

    // Deserialize all (ok for single-file method)
    let mut all_lists: Vec<NoSetListSerialized> = Vec::with_capacity(total);
    for i in 0..total {
        let archived_elem = archived.get(i).expect("index in range");
        let des: NoSetListSerialized = archived_elem.deserialize(&mut rkyv::Infallible).expect("deserialization");
        all_lists.push(des);
    }

    // Split into compacted chunk and remaining
    let take = std::cmp::min(all_lists.len(), batch_size as usize);
    let compact_chunk: Vec<NoSetListSerialized> = all_lists.drain(0..take).collect();
    let remaining: Vec<NoSetListSerialized> = all_lists; // moved remaining

    let source_size = target_size - 1;
    // Determine compacted filename: use last source batch = first_src here
    let is_full = (compact_chunk.len() as u64) >= batch_size;
    let compact_name = if is_full {
        format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}_compacted.rkyv", dir, source_size, first_src, target_size, next_compacted_idx)
    } else {
        format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", dir, source_size, first_src, target_size, next_compacted_idx)
    };

    test_print(&format!("   Writing compacted file {} ({} lists)", compact_name, compact_chunk.len().separated_string()));
    save_compacted_batch_atomic(&compact_name, &compact_chunk)?;

    // Now rewrite or delete the origin file with remaining lists
    if remaining.is_empty() {
        test_print(&format!("   Origin file {} emptied; deleting", filepath));
        let _ = std::fs::remove_file(&filepath);
    } else {
        test_print(&format!("   Origin file {} shrunk to {} lists; rewriting", filepath, remaining.len().separated_string()));
        // Serialize remaining and atomically replace original
        let mut serializer = AllocSerializer::<4096>::default();
        serializer.serialize_value(&remaining)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Serialization error: {:?}", e)))?;
        let bytes = serializer.into_serializer().into_inner();
        let path = Path::new(&filepath);
        let pid = std::process::id();
        let tmp = path.with_extension(format!("tmp.{}", pid));
        std::fs::write(&tmp, &bytes)?;
        let f = std::fs::File::open(&tmp)?;
        f.sync_all()?;
        // Replace
        if path.exists() { let _ = std::fs::remove_file(path); }
        std::fs::rename(&tmp, path)?;
    }

    test_print(&format!("   Single-file compaction finished: created {} (full={})", compact_name, is_full));
    Ok(())
}
