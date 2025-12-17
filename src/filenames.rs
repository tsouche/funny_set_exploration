use std::path::Path;
use std::fs;

/// Generate output filename with pattern:
/// nsl_{source_size:02}_batch_{source_batch:06}_to_{target_size:02}_batch_{target_batch:06}.rkyv
pub fn output_filename(
    base_path: &str,
    source_size: u8,
    source_batch: u32,
    target_size: u8,
    target_batch: u32,
) -> String {
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

/// Find input filename for reading by matching the pattern
/// *_to_{input_size}_batch_{target_batch}.rkyv and return the full path
/// input_size is the size of lists IN the file being read (not the size being created)
pub fn find_input_filename(base_path: &str, input_size: u8, target_batch: u32) -> Option<String> {
    let batch_width = 6;
    // input_size is already the size of lists in the file we're reading
    let pattern = format!("_to_{:02}_batch_{:0width$}.rkyv", input_size, target_batch, width = batch_width);
    crate::utils::test_print(&format!("   ... looking for input file matching: *{} in {}", pattern, base_path));

    let entries = match fs::read_dir(base_path) {
        Ok(e) => e,
        Err(err) => {
            crate::utils::debug_print(&format!("   ... ERROR: Cannot read directory {}: {}", base_path, err));
            return None;
        }
    };

    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with("nsl_") && name.ends_with(&pattern) {
                let found_path = entry.path().to_string_lossy().to_string();
                crate::utils::debug_print(&format!("   ... found: {}", name));
                return Some(found_path);
            }
        }
    }

    crate::utils::test_print("   ... no matching file found");
    None
}

/// Get next available output batch number by scanning filenames only.
/// Only considers files whose source batch is < `restart_batch`.
pub fn get_next_output_batch_from_files(base_path: &str, target_size: u8, restart_batch: u32) -> u32 {
    let entries = match fs::read_dir(base_path) {
        Ok(e) => e,
        Err(_) => return 0, // Directory doesn't exist, start from batch 0
    };

    let pattern_prefix = format!("_to_{:02}_batch_", target_size);
    let mut max_target_batch: Option<u32> = None;

    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with("nsl_") && name.contains(&pattern_prefix) && name.ends_with(".rkyv") {
                if let Some(to_pos) = name.find("_to_") {
                    let before_to = &name[..to_pos];
                    if let Some(batch_pos) = before_to.rfind("_batch_") {
                        let batch_str = &before_to[batch_pos + 7..];
                        if let Ok(source_batch_num) = batch_str.parse::<u32>() {
                            if source_batch_num < restart_batch {
                                let after_to = &name[to_pos + 4..];
                                if let Some(target_batch_pos) = after_to.rfind("_batch_") {
                                    let target_batch_str = &after_to[target_batch_pos + 7..after_to.len() - 5];
                                    if let Ok(target_batch_num) = target_batch_str.parse::<u32>() {
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
    crate::utils::debug_print(&format!("get_next_output_batch_from_files: next batch for size {:02} = {:06} (scanned filenames only)", target_size, next_batch));
    next_batch
}
