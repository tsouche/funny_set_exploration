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

    // Try atomic rename with retries (backoff) to handle transient Windows/SMB locks
    let mut renamed = false;
    for attempt in 0..5 {
        // Remove existing target (Windows requires remove before rename)
        if path.exists() { let _ = std::fs::remove_file(path); }
        match std::fs::rename(&tmp, path) {
            Ok(()) => { renamed = true; break; }
            Err(e) => {
                debug_print(&format!("atomic rename attempt {} failed for {}: {:?}", attempt + 1, filepath, e));
                std::thread::sleep(std::time::Duration::from_millis(200 * (attempt + 1)));
            }
        }
    }

    if renamed {
        return Ok(());
    }

    // If we reach here, rename failed persistently. Try fallback: write final file via serializer,
    // then remove the tmp file if fallback succeeded.
    debug_print(&format!("atomic rename failed persistently for {}; attempting fallback write", filepath));
    if crate::io_helpers::save_to_file_serialized(&lists_vec, filepath) {
        // fallback succeeded; remove tmp and return Ok
        let _ = std::fs::remove_file(&tmp);
        test_print(&format!("   Wrote compacted file (fallback) {}", filepath));
        return Ok(());
    }

    // Fallback also failed; leave tmp for inspection and return error
    debug_print(&format!("fallback write also failed for {} — leaving tmp file {} for inspection", filepath, tmp.display()));
    Err(std::io::Error::new(std::io::ErrorKind::Other, "Atomic rename and fallback write both failed"))
}

/// Compact exactly one batch in-place using the global count file as the plan.
/// - In-place only (input_dir == output_dir).
/// - No temp files: writes the compacted file directly with `save_to_file_serialized`.
/// - Idempotent: if the chosen compacted output already exists, the function exits without
///   modifying origins. After successfully creating the compacted file, intermediary and
///   global count files are updated to reflect the new state.
pub fn compact_size_files(input_dir: &str, output_dir: &str, target_size: u8, batch_size: u64) -> std::io::Result<()> {
    test_print(&format!("\nCompacting files for size {:02} (single batch)...", target_size));
    test_print(&format!("Target batch size: {} lists per file", batch_size.separated_string()));

    let start_time = std::time::Instant::now();

    if input_dir != output_dir {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Compaction is in-place only (input must equal output)"));
    }

    // Parse the consolidated global count file to build the ordered list of non-compacted inputs
    let report_filename = format!("{}/nsl_{:02}_global_count.txt", input_dir, target_size);
    let mut plan: Vec<(String, u64, u32, u32)> = Vec::new(); // (filename, count, src_batch, tgt_batch)
    if Path::new(&report_filename).exists() {
        use std::io::{BufRead, BufReader};
        let file = fs::File::open(&report_filename)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 4 { continue; }
            let src_tgt = parts[0].trim();
            let fields: Vec<&str> = src_tgt.split_whitespace().collect();
            if fields.len() < 2 { continue; }
            let (Ok(src), Ok(tgt)) = (fields[0].parse::<u32>(), fields[1].parse::<u32>()) else { continue; };
            let count_str = parts[2].trim();
            let digits_only: String = count_str.chars().filter(|c| c.is_ascii_digit()).collect();
            let Ok(count) = digits_only.parse::<u64>() else { continue; };
            let filename = parts[3].trim().to_string();
            let mut is_compacted = filename.contains("_compacted.rkyv");
            if parts.len() >= 5 {
                let flag = parts[4].trim().to_lowercase();
                if flag == "true" || flag == "yes" || flag == "1" { is_compacted = true; }
                if flag == "false" || flag == "no" || flag == "0" { is_compacted = false; }
            }
            if !is_compacted {
                plan.push((filename, count, src, tgt));
            }
        }
    }

    // If no plan from global count, nothing to do
    if plan.is_empty() {
        test_print("   No non-compacted entries found in global count; nothing to compact.");
        return Ok(());
    }

    // Order by target_batch then source_batch (ascending) to match previous behavior
    plan.sort_by(|a, b| match a.3.cmp(&b.3) { std::cmp::Ordering::Equal => a.2.cmp(&b.2), other => other });

    // Determine next compacted batch index by scanning existing compacted files
    let mut next_compact_idx: u32 = 0;
    if let Ok(entries) = std::fs::read_dir(output_dir) {
        let mut max_idx: Option<u32> = None;
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with("_compacted.rkyv") && name.contains(&format!("_to_{:02}_batch_", target_size)) {
                    if let Some(to_pos) = name.find("_to_") {
                        let after_to = &name[to_pos + 4..];
                        if let Some(batch_pos) = after_to.rfind("_batch_") {
                            let start = batch_pos + 7;
                            let end = after_to.len() - "_compacted.rkyv".len();
                            if end > start && end <= after_to.len() {
                                if let Ok(num) = after_to[start..end].parse::<u32>() {
                                    max_idx = Some(max_idx.map_or(num, |m| m.max(num)));
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(m) = max_idx { next_compact_idx = m + 1; }
    }

    // Accumulate lists up to batch_size
    let mut buffer: Vec<NoSetListSerialized> = Vec::new();
    let mut contribs: Vec<(u32, u64)> = Vec::new();
    let mut touched_files: Vec<(String, usize, usize, u32)> = Vec::new(); // (path, consumed, total, src_batch)
    let source_size = target_size - 1;
    const READ_CHUNK_SIZE: usize = 2_000_000;

    for (fname, _count, src_batch, _tgt_batch) in plan.iter() {
        if buffer.len() as u64 >= batch_size { break; }
        let path = format!("{}/{}", input_dir, fname);
        let mut all_lists = crate::io_helpers::load_lists_from_file(&path)?;
        let total = all_lists.len();
        let mut consumed = 0usize;

        while consumed < total && (buffer.len() as u64) < batch_size {
            let take = std::cmp::min(READ_CHUNK_SIZE, total - consumed);
            let chunk = &all_lists[consumed..consumed + take];
            let space_left = (batch_size as usize) - buffer.len();
            let take_now = std::cmp::min(space_left, chunk.len());
            buffer.extend_from_slice(&chunk[..take_now]);
            consumed += take_now;

            // track contribs
            if let Some(entry) = contribs.iter_mut().find(|e| e.0 == *src_batch) {
                entry.1 += take_now as u64;
            } else {
                contribs.push((*src_batch, take_now as u64));
            }
        }

        touched_files.push((path, consumed, total, *src_batch));
        if consumed > 0 {
            test_print(&format!("   Copied {:>10} lists from {}", consumed.separated_string(), fname));
        }
    }

    if buffer.is_empty() {
        test_print("   Nothing to compact (batch_size already met by existing compacts or no input)." );
        return Ok(());
    }

    // Determine output filename using the last contributor src batch
    let from_src = contribs.last().map(|c| c.0).unwrap_or(0);
    let is_full = (buffer.len() as u64) >= batch_size;
    let output_filename = if is_full {
        format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}_compacted.rkyv", output_dir, source_size, from_src, target_size, next_compact_idx)
    } else {
        format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", output_dir, source_size, from_src, target_size, next_compact_idx)
    };

    // Idempotent skip if already exists
    if Path::new(&output_filename).exists() {
        test_print(&format!("   Skipping existing compacted file {}", output_filename));
        return Ok(());
    }

    test_print(&format!("   Writing compacted file {} ({} lists)", output_filename, buffer.len().separated_string()));
    if !crate::io_helpers::save_to_file_serialized(&buffer, &output_filename) {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to write compacted file"));
    }

    // Apply deletions/rewrites after successful write
    for (path, consumed, total, _src_batch) in touched_files.iter() {
        if *consumed >= *total {
            test_print(&format!("   Origin file {} fully consumed; deleting", path));
            let _ = std::fs::remove_file(path);
        } else {
            test_print(&format!("   Origin file {} partially consumed; rewriting {} remaining lists", path, (*total - *consumed).separated_string()));
            let remaining_slice = &crate::io_helpers::load_lists_from_file(path)?; // reload to avoid moved ownership
            let remaining: Vec<NoSetListSerialized> = remaining_slice[*consumed..].to_vec();
            if !crate::io_helpers::save_to_file_serialized(&remaining, path) {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to rewrite origin file"));
            }
        }
    }

    // Update intermediaries for each source batch touched: remove consumed origins, add remaining entry if any
    for (path, consumed, total, src_batch) in touched_files.iter() {
        let origin_basename = Path::new(path).file_name().unwrap().to_string_lossy().into_owned();
        let inter_filename = format!("{}/nsl_{:02}_intermediate_count_from_{:02}_{:06}.txt", input_dir, target_size, source_size, src_batch);
        let mut new_lines: Vec<String> = Vec::new();
        if Path::new(&inter_filename).exists() {
            if let Ok(text) = std::fs::read_to_string(&inter_filename) {
                for line in text.lines() {
                    let l = line.trim();
                    if l.starts_with("...") {
                        let parts: Vec<&str> = l.split_whitespace().collect();
                        if parts.len() >= 5 {
                            let fname = parts[4];
                            if fname == origin_basename { continue; }
                        }
                        new_lines.push(line.to_string());
                    } else {
                        new_lines.push(line.to_string());
                    }
                }
            }
        }

        let remaining = if *consumed >= *total { 0usize } else { *total - *consumed };
        if remaining > 0 {
            new_lines.push(format!("   ... {:>8} lists in {}", remaining, origin_basename));
        }

        if !new_lines.is_empty() {
            let joined = new_lines.join("\n") + "\n";
            let _ = crate::io_helpers::write_text_atomic(Path::new(&inter_filename), &joined);
        } else {
            let _ = std::fs::remove_file(&inter_filename);
        }
    }

    // Add the new compacted file entry to the intermediary of the last contributing source batch
    let inter_new = format!("{}/nsl_{:02}_intermediate_count_from_{:02}_{:06}.txt", input_dir, target_size, source_size, from_src);
    let mut lines_new: Vec<String> = Vec::new();
    if Path::new(&inter_new).exists() {
        if let Ok(text) = std::fs::read_to_string(&inter_new) {
            for line in text.lines() { lines_new.push(line.to_string()); }
        }
    }
    lines_new.push(format!("   ... {:>8} lists in {}", buffer.len(), Path::new(&output_filename).file_name().unwrap().to_string_lossy()));
    let joined_new = lines_new.join("\n") + "\n";
    let _ = crate::io_helpers::write_text_atomic(Path::new(&inter_new), &joined_new);
    test_print("   Updated intermediary metadata files");

    // Regenerate global count after this single compacted file
    if let Err(e) = crate::list_of_nsl::count_size_files(input_dir, target_size, true, false) {
        return Err(e);
    }
    test_print("   Regenerated global count file");

    let elapsed = start_time.elapsed().as_secs_f64();
    test_print(&format!("\nCompaction completed in {:.2} seconds", elapsed));
    test_print(&format!("   Compacted file: {}", output_filename));
    test_print(&format!("   Lists in compacted file: {}", buffer.len().separated_string()));

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

    // Ensure mmap and file are dropped so Windows allows rewrite/rename of the origin file
    drop(mmap);
    drop(file);

    let source_size = target_size - 1;
    // Determine compacted filename: use last source batch = first_src here
    let is_full = (compact_chunk.len() as u64) >= batch_size;
    let compact_name = if is_full {
        format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}_compacted.rkyv", dir, source_size, first_src, target_size, next_compacted_idx)
    } else {
        format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", dir, source_size, first_src, target_size, next_compacted_idx)
    };

    test_print(&format!("   Writing compacted file {} ({} lists)", compact_name, compact_chunk.len().separated_string()));
    // Use the simpler save helper here to avoid platform rename permission issues during tests.
    if !crate::io_helpers::save_to_file_serialized(&compact_chunk, &compact_name) {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to save compacted file"));
    }

    // Now rewrite or delete the origin file with remaining lists
    if remaining.is_empty() {
        test_print(&format!("   Origin file {} emptied; deleting", filepath));
        let _ = std::fs::remove_file(&filepath);
    } else {
        test_print(&format!("   Origin file {} shrunk to {} lists; rewriting", filepath, remaining.len().separated_string()));
        // Use simpler save helper to rewrite origin (avoid Windows locking/permission race in tests)
        if !crate::io_helpers::save_to_file_serialized(&remaining, &filepath) {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed to save rewritten origin file"));
        }
    }

    test_print(&format!("   Single-file compaction finished: created {} (full={})", compact_name, is_full));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io_helpers;
    use std::fs;
    use std::path::Path;

    fn make_test_dir(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("funny_test_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("create temp dir");
        p.to_string_lossy().into_owned()
    }

    fn eq_nsl(a: &NoSetListSerialized, b: &NoSetListSerialized) -> bool {
        a.n == b.n && a.max_card == b.max_card && a.no_set_list == b.no_set_list && a.remaining_cards_list == b.remaining_cards_list
    }

    #[test]
    fn compact_one_file_preserves_lists_no_loss_no_dup() {
        let dir = make_test_dir("onefile");
        // Create 5 distinct NoSetListSerialized entries
        let lists: Vec<NoSetListSerialized> = (0..5).map(|i| NoSetListSerialized {
            n: 3,
            max_card: i as usize,
            no_set_list: vec![i as usize, i as usize + 1, i as usize + 2],
            remaining_cards_list: vec![i as usize + 3, i as usize + 4],
        }).collect();

        let filename = format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}.rkyv", dir, 14u8, 0u32, 15u8, 0u32);
        assert!(io_helpers::save_to_file_serialized(&lists, &filename));

        // Run single-file compaction with batch_size = 3 (will take first 3 -> compacted)
        compact_one_file_inplace(&dir, 15u8, 3).expect("compaction failed");

        // Expect compacted file (index 000000) exists
        let compacted_path = format!("{}/nsl_{:02}_batch_{:06}_to_{:02}_batch_{:06}_compacted.rkyv", dir, 14u8, 0u32, 15u8, 0u32);
        assert!(Path::new(&compacted_path).exists(), "compacted file missing");

        let compacted = io_helpers::read_from_file_serialized(&compacted_path).expect("read compacted");
        assert_eq!(compacted.len(), 3);

        // Origin should have remaining 2 lists
        let origin = io_helpers::read_from_file_serialized(&filename).expect("read origin");
        assert_eq!(origin.len(), 2);

        // Combine and verify all original lists are present exactly once
        let mut combined: Vec<NoSetListSerialized> = Vec::new();
        combined.extend(compacted.iter().cloned());
        combined.extend(origin.iter().cloned());
        assert_eq!(combined.len(), lists.len());

        for orig in lists {
            let mut found = 0usize;
            for c in &combined {
                if eq_nsl(&orig, c) { found += 1; }
            }
            assert_eq!(found, 1, "Original list not found exactly once");
        }

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }
}
