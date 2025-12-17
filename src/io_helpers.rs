use std::fs::File;
use std::io::{self, Write};
use memmap2::Mmap;
use rkyv::check_archived_root;
use rkyv::Deserialize;

use crate::no_set_list::NoSetListSerialized;

/// Save a vector of `NoSetListSerialized` using rkyv to `filename`.
/// Returns true on success, false on error (legacy API retained).
pub fn save_to_file_serialized(list: &Vec<NoSetListSerialized>, filename: &str) -> bool {
    debug_print(&format!("save_to_file_serialized: Serializing {} n-lists to {} using rkyv", list.len(), filename));

    let bytes = match rkyv::to_bytes::<_, 256>(list) {
        Ok(b) => b,
        Err(e) => {
            debug_print(&format!("save_to_file_nlist: Error serializing: {}", e));
            return false;
        }
    };

    match std::fs::write(filename, bytes) {
        Ok(_) => {
            debug_print(&format!("save_to_file_nlist: Saved {} n-lists to {}", list.len(), filename));
            true
        }
        Err(e) => {
            debug_print(&format!("save_to_file_nlist: Error writing {}: {}", filename, e));
            false
        }
    }
}

/// Read a vector of `NoSetListSerialized` from `filename` using memory mapping and rkyv.
/// Returns `Some(vec)` on success, `None` on error.
pub fn read_from_file_serialized(filename: &str) -> Option<Vec<NoSetListSerialized>> {
    debug_print(&format!("read_from_file_serialized: Loading n-lists from {} using rkyv", filename));

    let file = match File::open(filename) {
        Ok(f) => f,
        Err(e) => {
            debug_print(&format!("read_from_file_nlist: Error opening {}: {}", filename, e));
            return None;
        }
    };

    let mmap = unsafe {
        match Mmap::map(&file) {
            Ok(m) => m,
            Err(e) => {
                debug_print(&format!("read_from_file_nlist: Error mapping {}: {}", filename, e));
                return None;
            }
        }
    };

    match check_archived_root::<Vec<NoSetListSerialized>>(&mmap) {
        Ok(archived_vec) => {
            let deserialized: Vec<NoSetListSerialized> = archived_vec
                .deserialize(&mut rkyv::Infallible)
                .expect("Deserialization should not fail after validation");
            debug_print(&format!("read_from_file_serialized: deserialized {} n-lists", deserialized.len()));
            Some(deserialized)
        }
        Err(e) => {
            debug_print(&format!("read_from_file_serialized: Validation error for {}: {:?}", filename, e));
            None
        }
    }
}

/// Load lists from a file path and return io::Result<Vec<NoSetListSerialized>> (uses rkyv + mmap)
pub fn load_lists_from_file(filepath: &str) -> io::Result<Vec<NoSetListSerialized>> {
    let file = File::open(filepath)?;
    let mmap = unsafe { Mmap::map(&file)? };

    match check_archived_root::<Vec<NoSetListSerialized>>(&mmap[..]) {
        Ok(archived_lists) => {
            let lists: Vec<NoSetListSerialized> = archived_lists
                .deserialize(&mut rkyv::Infallible)
                .expect("Deserialization should never fail with Infallible");
            Ok(lists)
        }
        Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, format!("Archive validation failed: {:?}", e))),
    }
}

/// Atomically write text to `path` by writing a temp file, fsyncing, then renaming into place.
pub fn write_text_atomic(path: &std::path::Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// Minimal debug_print to mirror crate function expectations when used from this module
fn debug_print(s: &str) {
    crate::utils::debug_print(s);
}
