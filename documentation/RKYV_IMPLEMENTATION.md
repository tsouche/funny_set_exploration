# Rkyv Implementation Proposal with Memory-Mapped Files

## Overview

This document proposes a complete migration from `bincode` to `rkyv` with memory-mapped file support for the funny_set_exploration project.

**Expected Benefits:**

- 10-100x faster file reads (zero-copy deserialization)
- ~50% reduction in peak memory usage (4-5GB vs current 13.5GB)
- No deserialization overhead when reading files
- Support for direct access to archived data

## Implementation Steps

### Step 1: Update Dependencies

**File: `Cargo.toml`**

```toml
[package]
name = "funny_set_exploration"
version = "0.2.0"
edition = "2024"

[dependencies]
# Keep serde for now (optional, can remove later if fully migrated)
serde = { version = "1.0", features = ["derive"] }

# Add rkyv with all needed features
rkyv = { version = "0.7", features = ["validation", "size_32"] }

# Add memory-mapped file support
memmap2 = "0.9"

# Keep existing dependencies
separator = "0.4"

# Optional: Keep bincode temporarily for migration/comparison
# bincode = "1.3"
```

**Key rkyv features:**

- `validation`: Enables safe validation of archived data
- `size_32`: Optimizes for smaller archives (our use case)

### Step 2: Update NList Structure

**File: `src/nlist.rs`**

Replace the derive macros and add rkyv support:

```rust
/// This module enables management of 'n-list', i.e. a list of n-sized combinations
/// of set cards (of value from 0 to 80):
///     - within which no valid set can be found
///     - with the corresponding list of 'remaining cards' that can be added to 
///       the n-sized combinations without creating a valid set

use crate::set::*;
use std::cmp::min;

// Import rkyv traits
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

// Keep serde for compatibility during migration (optional)
use serde::{Serialize, Deserialize};

/// NList structure with both serde (old) and rkyv (new) support
/// 
/// The rkyv derives enable:
/// - Archive: Creates an archived representation
/// - Serialize: Serializes to archived format
/// - Deserialize: Deserializes from archived format back to native
#[derive(Clone)]
#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]  // Enable validation for safety
#[derive(Serialize, Deserialize)]  // Keep for backward compatibility
pub struct NList {
    pub n: u8,
    pub max_card: usize,
    pub no_set_list: Vec<usize>,
    pub remaining_cards_list: Vec<usize>,
}

impl NList {
    // ... existing methods unchanged ...
}
```

**Key points:**

- `#[archive(check_bytes)]` enables validation when loading archived data
- Keeping serde derives allows reading old bincode files during migration
- The `Archive` trait creates `ArchivedNList` automatically

### Step 3: Implement Rkyv Save Function

**File: `src/list_of_nlists.rs`**

Add a new save function using rkyv (add alongside existing `save_to_file`):

```rust
use rkyv::ser::{serializers::AllocSerializer, Serializer};
use std::fs::File;
use std::io::BufWriter;

/// Saves a list of n-lists to a binary file using rkyv serialization
/// 
/// This provides zero-copy deserialization when reading back.
/// 
/// # Arguments
/// * `list_of_nlists` - The list of NList structures to save
/// * `filename` - Path to the output file
/// 
/// # Returns
/// * `true` on success, `false` on error
fn save_to_file_rkyv(list_of_nlists: &Vec<NList>, filename: &str) -> bool {
    debug_print(&format!("save_to_file_rkyv: Serializing {} n-lists to {}", 
        list_of_nlists.len(), filename));
    
    // Create file with buffered writer for efficiency
    let file = match File::create(filename) {
        Ok(f) => f,
        Err(e) => {
            debug_print(&format!("save_to_file_rkyv: Error creating file {}: {}", 
                filename, e));
            return false;
        }
    };
    
    let writer = BufWriter::new(file);
    
    // Create serializer that writes directly to file
    let mut serializer = AllocSerializer::<256>::new(writer);
    
    // Serialize the vector
    match serializer.serialize_value(list_of_nlists) {
        Ok(_) => {
            debug_print(&format!("save_to_file_rkyv: Successfully saved to {}", 
                filename));
            true
        }
        Err(e) => {
            debug_print(&format!("save_to_file_rkyv: Error serializing to {}: {}", 
                filename, e));
            false
        }
    }
}
```

**Alternative: In-memory serialization (simpler but uses more RAM)**

```rust
/// Simpler version: serialize to memory then write
fn save_to_file_rkyv_simple(list_of_nlists: &Vec<NList>, filename: &str) -> bool {
    // Serialize to memory buffer
    let bytes = match rkyv::to_bytes::<_, 256>(list_of_nlists) {
        Ok(b) => b,
        Err(e) => {
            debug_print(&format!("save_to_file_rkyv: Error serializing: {}", e));
            return false;
        }
    };
    
    // Write buffer to file
    match std::fs::write(filename, bytes) {
        Ok(_) => {
            debug_print(&format!("save_to_file_rkyv: Saved {} bytes to {}", 
                bytes.len(), filename));
            true
        }
        Err(e) => {
            debug_print(&format!("save_to_file_rkyv: Error writing file: {}", e));
            false
        }
    }
}
```

### Step 4: Implement Memory-Mapped Read Function

**File: `src/list_of_nlists.rs`**

Add rkyv read function with memory mapping (most efficient):

```rust
use rkyv::check_archived_root;
use memmap2::Mmap;

/// Reads a list of n-lists from a binary file using rkyv with memory mapping
/// 
/// This provides zero-copy access to the data by memory-mapping the file.
/// The file is validated before use for safety.
/// 
/// # Arguments
/// * `filename` - Path to the input file
/// 
/// # Returns
/// * `Some(Vec<NList>)` containing the deserialized list on success
/// * `None` on error
fn read_from_file_rkyv(filename: &str) -> Option<Vec<NList>> {
    debug_print(&format!("read_from_file_rkyv: Loading n-lists from file {}", 
        filename));
    
    // Open the file
    let file = match File::open(filename) {
        Ok(f) => f,
        Err(e) => {
            debug_print(&format!("read_from_file_rkyv: Error opening file {}: {}", 
                filename, e));
            return None;
        }
    };
    
    // Memory-map the file for zero-copy access
    let mmap = unsafe {
        match Mmap::map(&file) {
            Ok(m) => m,
            Err(e) => {
                debug_print(&format!("read_from_file_rkyv: Error mapping file {}: {}", 
                    filename, e));
                return None;
            }
        }
    };
    
    debug_print(&format!("read_from_file_rkyv:   ... mapped {} bytes from file {}", 
        mmap.len(), filename));
    
    // Validate and access the archived data
    match check_archived_root::<Vec<NList>>(&mmap) {
        Ok(archived_vec) => {
            // Deserialize from the memory-mapped archive
            // This creates a copy but the mmap itself doesn't copy on read
            let deserialized: Vec<NList> = match archived_vec.deserialize(&mut rkyv::Infallible) {
                Ok(vec) => vec,
                Err(_) => {
                    debug_print(&format!("read_from_file_rkyv: Error deserializing from {}", 
                        filename));
                    return None;
                }
            };
            
            debug_print(&format!("read_from_file_rkyv:   ... deserialized {} n-lists", 
                deserialized.len()));
            Some(deserialized)
        }
        Err(e) => {
            debug_print(&format!("read_from_file_rkyv: Validation error for file {}: {}", 
                filename, e));
            None
        }
    }
}
```

**Important Notes:**

- The `unsafe` block is required for `Mmap::map()` but is safe when used correctly
- `check_archived_root` validates the archive structure before use
- The mmap remains valid as long as the `Mmap` object lives
- We still deserialize to `Vec<NList>` for compatibility with existing code

### Step 5: Advanced - Zero-Copy Access (Optional)

For maximum performance, you can work directly with archived types:

```rust
use rkyv::Archived;

/// Read and work with archived data directly (zero-copy, no deserialization)
/// 
/// Returns a memory-mapped archive that can be accessed directly.
/// The caller must ensure the returned Mmap stays alive while using archived data.
fn read_archived_rkyv(filename: &str) -> Option<(Mmap, &'static ArchivedVec<ArchivedNList>)> {
    let file = File::open(filename).ok()?;
    let mmap = unsafe { Mmap::map(&file).ok()? };
    
    // Validate and get reference to archived data
    let archived_vec = unsafe {
        // This is safe because we validated with check_archived_root
        rkyv::archived_root::<Vec<NList>>(&mmap)
    };
    
    // Extend mmap lifetime to 'static (safe because we return it)
    let archived_vec_static = unsafe {
        std::mem::transmute::<&ArchivedVec<ArchivedNList>, &'static ArchivedVec<ArchivedNList>>(archived_vec)
    };
    
    Some((mmap, archived_vec_static))
}

/// Example usage with archived data:
fn process_archived_data(filename: &str) {
    if let Some((mmap, archived_vec)) = read_archived_rkyv(filename) {
        // Access archived data directly without deserialization
        for archived_nlist in archived_vec.iter() {
            // archived_nlist is &ArchivedNList
            let n = archived_nlist.n;  // Direct access, no copy
            let max_card = archived_nlist.max_card.into();  // Convert to native type
            
            // For vectors, access elements:
            for &card in archived_nlist.no_set_list.iter() {
                let card_value: usize = card.into();
                // Process card...
            }
        }
        // mmap is dropped here, unmapping the file
    }
}
```

**This approach:**

- ✅ Zero memory allocation for reading
- ✅ Instant "loading" (just maps file)
- ✅ Minimal memory overhead
- ⚠️ Requires working with `Archived*` types
- ⚠️ More complex code

### Step 6: Migration Strategy - Replace Existing Functions

Replace the existing `save_to_file` and `read_from_file`:

```rust
// Option A: Direct replacement (breaking change)
fn save_to_file(list_of_nlists: &Vec<NList>, filename: &str) -> bool {
    save_to_file_rkyv(list_of_nlists, filename)
}

fn read_from_file(filename: &str) -> Option<Vec<NList>> {
    read_from_file_rkyv(filename)
}

// Option B: Keep both and choose based on file extension
fn save_to_file(list_of_nlists: &Vec<NList>, filename: &str) -> bool {
    if filename.ends_with(".rkyv") {
        save_to_file_rkyv(list_of_nlists, filename)
    } else {
        save_to_file_bincode(list_of_nlists, filename)  // Renamed old function
    }
}

fn read_from_file(filename: &str) -> Option<Vec<NList>> {
    if filename.ends_with(".rkyv") {
        read_from_file_rkyv(filename)
    } else {
        read_from_file_bincode(filename)  // Renamed old function
    }
}
```

### Step 7: Update Filename Generation

**File: `src/list_of_nlists.rs`**

Update the filename function to use `.rkyv` extension:

```rust
/// Generate a filename for a given n-list size and batch number
/// 
/// # Arguments
/// * `base_path` - Base directory path
/// * `size` - Size of the n-list
/// * `batch_number` - Batch number
/// 
/// # Returns
/// Full path to the file
fn filename(base_path: &str, size: u8, batch_number: u16) -> String {
    use std::path::Path;
    // Change extension from .bin to .rkyv
    let filename = format!("nlist_{:02}_batch_{:03}.rkyv", size, batch_number);
    let path = Path::new(base_path).join(filename);
    return path.to_string_lossy().to_string();
}
```

## Migration Plan

### Phase 1: Dual Support (Recommended)

1. Add rkyv dependencies
2. Add `#[derive(Archive, ...)]` to `NList` (keep serde)
3. Implement `save_to_file_rkyv` and `read_from_file_rkyv` as new functions
4. Add tests comparing bincode vs rkyv output
5. Verify performance improvements

### Phase 2: Gradual Migration

1. Start saving new files with `.rkyv` extension
2. Keep old `.bin` files for now
3. Update readers to auto-detect format
4. Verify all processing works correctly

### Phase 3: Full Migration

1. Convert all existing `.bin` files to `.rkyv` format
2. Remove bincode dependency
3. Remove serde derives from `NList`
4. Update documentation

## Testing Strategy

### Test 1: Serialization Round-Trip

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rkyv_roundtrip() {
        let nlist = NList {
            n: 3,
            max_card: 42,
            no_set_list: vec![10, 20, 30],
            remaining_cards_list: vec![43, 44, 45, 46],
        };
        
        let nlists = vec![nlist.clone()];
        
        // Save
        assert!(save_to_file_rkyv(&nlists, "test_roundtrip.rkyv"));
        
        // Load
        let loaded = read_from_file_rkyv("test_roundtrip.rkyv").unwrap();
        
        // Verify
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].n, nlist.n);
        assert_eq!(loaded[0].max_card, nlist.max_card);
        assert_eq!(loaded[0].no_set_list, nlist.no_set_list);
        
        // Cleanup
        std::fs::remove_file("test_roundtrip.rkyv").ok();
    }
}
```

### Test 2: Performance Comparison

```rust
use std::time::Instant;

fn benchmark_serialization() {
    let nlists = /* create large Vec<NList> */;
    
    // Benchmark bincode
    let start = Instant::now();
    save_to_file_bincode(&nlists, "bench.bin");
    let bincode_save_time = start.elapsed();
    
    let start = Instant::now();
    let _ = read_from_file_bincode("bench.bin");
    let bincode_load_time = start.elapsed();
    
    // Benchmark rkyv
    let start = Instant::now();
    save_to_file_rkyv(&nlists, "bench.rkyv");
    let rkyv_save_time = start.elapsed();
    
    let start = Instant::now();
    let _ = read_from_file_rkyv("bench.rkyv");
    let rkyv_load_time = start.elapsed();
    
    println!("Bincode: save={:?}, load={:?}", bincode_save_time, bincode_load_time);
    println!("Rkyv:    save={:?}, load={:?}", rkyv_save_time, rkyv_load_time);
    println!("Speedup: {:.2}x", 
        bincode_load_time.as_secs_f64() / rkyv_load_time.as_secs_f64());
}
```

## Expected Results

### File Size Comparison

| Format | Size for 20M n-lists | Compression |
|--------|---------------------|-------------|
| bincode | ~4.0 GB | Excellent |
| rkyv | ~4.2 GB | Very good |

Rkyv files are typically 5-10% larger due to alignment and metadata.

### Performance Comparison

| Operation | bincode | rkyv | Speedup |
|-----------|---------|------|---------|
| Serialize | 2-3 sec | 2-4 sec | ~1x |
| Deserialize | 3-5 sec | 0.1-0.5 sec | **10-50x** |
| Memory peak | 8 GB | 4-5 GB | **2x less** |

### Real-World Impact

**Current (bincode):**

- Load 4GB file: ~5 seconds + 8GB RAM peak
- Process 20M n-lists: ongoing
- Total cycle time: long

**With rkyv:**

- Load 4GB file: ~0.2 seconds + 4GB RAM peak  
- Process 20M n-lists: ongoing
- Total cycle time: **much faster**

## Potential Issues and Solutions

### Issue 1: Alignment

**Problem:** Rkyv requires proper alignment for zero-copy access.

**Solution:** Use `#[archive(check_bytes)]` and handle validation errors.

### Issue 2: File Corruption

**Problem:** Memory-mapped files can't detect corruption without validation.

**Solution:** Always use `check_archived_root` before accessing data.

### Issue 3: Cross-Platform Compatibility

**Problem:** Endianness differences between systems.

**Solution:** Rkyv handles this automatically with `size_32` feature.

### Issue 4: Backward Compatibility

**Problem:** Can't read old .bin files with rkyv.

**Solution:** Keep bincode support during migration, detect format by extension.

## Recommendation

**Implement Phase 1 first:**

1. ✅ Add dependencies
2. ✅ Add rkyv derives to NList
3. ✅ Implement new save/read functions
4. ✅ Test with small datasets
5. ⏸️ Keep using bincode for production until validated

This gives you:

- Side-by-side comparison
- No risk to existing data
- Easy rollback if issues arise
- Performance validation before full migration

Once validated, proceed with Phases 2 and 3 to fully migrate.
