# Technical Documentation

## Architecture Overview (v0.4.12)

This document provides technical details about the implementation of the funny_set_exploration algorithm.

**Current Version:** 0.4.12 - Hybrid stack/heap implementation with auto-compaction workflow for sizes 13+

## Core Algorithm

### Incremental List Generation

The algorithm builds progressively larger no-set combinations:

1. **Seed Generation (3-card lists)**:
   - Enumerate all 3-card combinations where indices i < j < k < 72
   - For each combination, verify no valid set exists
   - Compute remaining cards that won't form sets with any pair
   - Store as `NList` structures

2. **Expansion (n-card to n+1-card)**:
   - For each n-card list, try adding each remaining card
   - Update forbidden cards (cards that would form sets)
   - If enough cards remain to reach target size (12/15/18), save the n+1-list
   - Otherwise, prune that branch

### Key Optimization

**Forward-only search**: Since card order doesn't matter, we only consider cards with values > current maximum. This eliminates redundant combinations and dramatically reduces search space.

**Early pruning**: If an n-list doesn't have enough remaining cards to reach 12 cards, we abandon that branch immediately.

## Data Structures

### NList Structure

```rust
#[derive(Clone, Serialize, Deserialize)]
pub struct NList {
    pub n: u8,                           // Number of cards in combination
    pub max_card: usize,                 // Highest card value in no_set_list
    pub no_set_list: Vec<usize>,         // The n cards forming no-set
    pub remaining_cards_list: Vec<usize> // Cards that can extend this list
}
```

**Fields explained:**

- `n`: Size of the current combination (3 to 18)
- `max_card`: Used for filtering - only consider cards > max_card
- `no_set_list`: The actual card indices (0-80) forming a valid no-set
- `remaining_cards_list`: Pre-computed list of safe cards to add

### ListOfNlist Structure

```rust
#[derive(Serialize, Deserialize)]
pub struct ListOfNlist {
    pub current_size: u8,          // Size of n-lists being processed
    pub current: Vec<NList>,       // Current batch of n-lists
    pub current_file_count: u16,   // Files processed for current size
    pub current_list_count: u64,   // Total n-lists processed
    pub new: Vec<NList>,           // Generated n+1-lists
    pub new_file_count: u16,       // Files saved for new size
    pub new_list_count: u64,       // Total n+1-lists created
    #[serde(skip)]
    pub base_path: String,         // Output directory
}
```

**Processing flow:**

1. Load batch of n-lists into `current`
2. Generate n+1-lists, accumulate in `new`
3. When `new` reaches `MAX_NLISTS_PER_FILE`, save to disk and clear
4. Move to next batch of n-lists

## File I/O

### Serialization

Currently using **bincode** (compact binary format):

```rust
fn save_to_file(list: &Vec<NList>, filename: &str) -> bool {
    let encoded = bincode::serialize(list)?;
    std::fs::write(filename, encoded)?;
    Ok(())
}

fn read_from_file(filename: &str) -> Option<Vec<NList>> {
    let bytes = std::fs::read(filename).ok()?;
    bincode::deserialize(&bytes).ok()
}
```

**Trade-offs:**

- ✅ Simple API
- ✅ Good compression
- ✅ Cross-platform
- ⚠️ Requires full deserialization (memory copy)
- ⚠️ Peak memory = 2x file size during load

### File Naming Convention

Pattern: `nlist_{size:02}_batch_{number:06}.bin`

Examples:

- `nlist_03_batch_000000.bin` - First batch of 3-card lists  
- `nlist_07_batch_000042.bin` - 43rd batch of 7-card lists

### Path Configuration

Files can be written to custom directories:

```rust
// Default: current directory
let mut lists = ListOfNlist::new();

// Custom path
let mut lists = ListOfNlist::with_path("/path/to/storage");
```

The `filename()` function constructs full paths using `std::path::Path::join()`.

## Memory Management

### Batch Processing Strategy

**Problem**: Full dataset doesn't fit in RAM

- 6-card lists: ~156M combinations
- 7-card lists: Estimated billions
- Cannot hold all in memory simultaneously

**Solution**: Batching

- Process input in batches of `MAX_NLISTS_PER_FILE` (default: 20M)
- Generate n+1-lists incrementally
- Save output batches as `new` vector fills up
- Clear `new` after each save

### Memory Profile

**During processing:**

```
Current n-lists in RAM:   ~0-20M lists
New n+1-lists in RAM:     ~0-20M lists  
Peak usage:               ~13.5GB (when saving batch)
```

**After save:**

```
Current n-lists:          ~0-20M lists
New n+1-lists:            0 (cleared)
Baseline usage:           ~5GB
```

### Optimization Opportunity

Switching to **rkyv** could reduce peak memory:

- Zero-copy deserialization
- Memory-map files instead of loading
- Estimated peak: ~4-5GB (vs current 13.5GB)

## Algorithm Complexity

### Space Complexity

**Per n-list:**

- Fixed fields: ~24 bytes
- `no_set_list`: n × 8 bytes
- `remaining_cards_list`: variable, typically (81 - max_card) × 8 bytes

**Total space for batch:**

- 20M n-lists × ~200 bytes ≈ 4GB

### Time Complexity

**For generating n+1-lists from n-lists:**

For each n-list:

- Iterate through remaining cards: O(remaining_count)
- For each candidate card:
  - Check against existing n cards: O(n)
  - Update forbidden list: O(n)
  
Total: O(num_nlists × remaining × n)

**Observed growth:**

- 3-cards: 58,896 (instant)
- 4-cards: 1,098,240 (seconds)
- 5-cards: 13,394,538 (minutes)
- 6-cards: 155,769,345 (hours)
- 7-cards: Expected billions (days)

Growth rate appears exponential initially, but prune rate increases as remaining cards decrease.

## Set Validation Logic

### Core Set Check

```rust
pub fn is_set(i0: usize, i1: usize, i2: usize) -> bool {
    // Three cards form a set if for each attribute,
    // values are all same or all different
    for shift in [0, 2, 4, 6] {
        let mask = 0b11 << shift;
        let a0 = (i0 & mask) >> shift;
        let a1 = (i1 & mask) >> shift;
        let a2 = (i2 & mask) >> shift;
        
        // Valid if all same or all different
        if !((a0 == a1 && a1 == a2) || 
             (a0 != a1 && a1 != a2 && a0 != a2)) {
            return false;
        }
    }
    true
}
```

**Card encoding**: Each card 0-80 encodes 4 attributes (2 bits each):

- Bits 0-1: Attribute 1 (0-2)
- Bits 2-3: Attribute 2 (0-2)
- Bits 4-5: Attribute 3 (0-2)
- Bits 6-7: Attribute 4 (0-2)

Total combinations: 3^4 = 81 cards

### Computing Third Card

Given two cards, compute the third card that would form a set:

```rust
pub fn next_to_set(i0: usize, i1: usize) -> usize {
    let mut i2 = 0;
    for shift in [0, 2, 4, 6] {
        let mask = 0b11 << shift;
        let a0 = (i0 & mask) >> shift;
        let a1 = (i1 & mask) >> shift;
        
        // Compute a2 such that set rule is satisfied
        let a2 = if a0 == a1 {
            a0  // All same
        } else {
            3 - a0 - a1  // All different
        };
        i2 |= a2 << shift;
    }
    i2
}
```

This is much faster than checking all possible third cards.

## Performance Metrics

### Observed Performance

**Hardware dependent**, typical results on modern CPU:

| Size | Count | Time | RAM Peak |
|------|-------|------|----------|
| 3 | 58,896 | <1s | <1GB |
| 4 | 1.1M | <10s | ~2GB |
| 5 | 13.4M | ~1min | ~5GB |
| 6 | 155.8M | ~1hr | ~13GB |
| 7 | TBD | hours+ | ~13GB |

### Bottlenecks

1. **Disk I/O**: Writing 4GB files takes time
2. **Memory allocation**: Creating millions of vectors
3. **Forbidden card computation**: O(n) per candidate

### Future Optimization Strategies

1. **Parallelization**:
   - Independent n-lists can be processed in parallel
   - Thread-per-batch or rayon parallel iterators
   - Estimated speedup: 4-8x on modern CPUs

2. **SIMD**:
   - Vectorize set validation checks
   - Batch forbidden card computations
   - Estimated speedup: 2-4x for validation

3. **GPU Acceleration**:
   - Offload set validation to GPU
   - Process millions of candidates simultaneously
   - Estimated speedup: 10-100x for validation

4. **Better Data Structures**:
   - Use BitVec for remaining cards (81 bits vs 8 bytes per card)
   - Pack NList more efficiently
   - Estimated space saving: 50%

## Testing Strategy

### Current Testing

Minimal automated testing - mostly validation through:

- Known counts for small n (3-6 cards)
- Invariant checks (no sets in output)
- File I/O round-trip tests

### Recommended Testing

1. **Unit tests**:
   - Set validation logic
   - next_to_set correctness
   - NList generation for small examples

2. **Integration tests**:
   - Full pipeline for n=3,4
   - File serialization round-trips
   - Path configuration

3. **Property tests**:
   - No set exists in any generated n-list
   - All remaining cards are valid
   - Exhaustiveness (all valid n-lists generated)

## Debugging Tools

### Debug Printing

Controlled via utils.rs:

```rust
debug_print_on();   // Enable debug output
debug_print_off();  // Disable debug output
test_print_on();    // Enable test/progress output
```

### Progress Tracking

Built into processing loop:

- Current file being processed
- New batches saved
- Total counts with thousand separators

Example output:

```
Start processing files for size 7:
   ... saved 20,000,001 n-lists to nlist_07_batch_000000.bin
   ... saved 20,000,001 n-lists to nlist_07_batch_000001.bin
```

## Build Configuration

### Dependencies

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
bincode = "1.3"
separator = "0.4"
```

### Compilation

**Debug build** (with symbols, slower):

```bash
cargo build
```

**Release build** (optimized, ~10x faster):

```bash
cargo build --release
```

**Important**: Always use `--release` for production runs!

### Rust Edition

Using 2024 edition for latest language features.

## Known Issues

1. **Windows linker requirement**:
   - MSVC linker needed OR use GNU toolchain
   - Run `rustup default stable-gnu` if MSVC not available

2. **Memory usage warnings**:
   - 13.5GB peak is expected for default batch size
   - Reduce `MAX_NLISTS_PER_FILE` if RAM limited

3. **No progress persistence**:
   - Interrupting requires restart from last completed size
   - Files already saved are not regenerated

## Future Technical Improvements

See CHANGELOG.md "Future Considerations" section for planned enhancements including:

- rkyv migration for zero-copy deserialization
- Parallel processing support
- Checkpoint/resume capability
- Compression options
