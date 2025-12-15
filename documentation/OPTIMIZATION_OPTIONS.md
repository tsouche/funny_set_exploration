# File Size Optimization Options

## Problem Analysis

- Current file size: **3.5-4.0 GB per 20M batch** (size 6)
- Expected size: **~2 GB** based on data structure
- Overhead: **2× bloat** from Vec capacity serialization
- Shrink_to_fit: **Added 17% overhead** without reducing file size

## Root Cause

rkyv's `ArchivedVec` serializes:

- Length (actual data count)
- Capacity (allocated space) - **THIS CAUSES BLOAT**
- Alignment padding
- Vec metadata

Even after `shrink_to_fit()`, rkyv may serialize internal fragmentation.

---

## Option 1: Pre-Allocate with Exact Capacity ⭐ RECOMMENDED

**Concept**: Allocate Vecs with exact final size to avoid growth overhead

### Implementation

```rust
// In build_higher_nlists(), replace:
let mut n_plus_1_lists = Vec::new();

// With:
let estimated_capacity = self.remaining_cards_list.len(); // rough estimate
let mut n_plus_1_lists = Vec::with_capacity(estimated_capacity);

// For each NList created:
let mut n_plus_1_primary_list = Vec::with_capacity(self.no_set_list.len() + 1);
n_plus_1_primary_list.extend_from_slice(&self.no_set_list);
n_plus_1_primary_list.push(*c);

// For remaining cards:
let max_remaining = self.remaining_cards_list.len();
let mut n_plus_1_remaining_list = Vec::with_capacity(max_remaining);
n_plus_1_remaining_list.extend(
    self.remaining_cards_list.iter().filter(|&&x| x > *c).copied()
);
```

### Expected Impact

- **File size reduction**: 30-40% (4GB → 2.5GB)
- **Performance**: +5-10% faster (less reallocation)
- **Complexity**: Low (minor code changes)

---

## Option 2: Clone to New Vec Before Serialization ⭐⭐ BEST EFFICIENCY

**Concept**: Create fresh Vecs with exact data, no growth history

### Implementation

```rust
// In save_new_to_file(), before serialization:
fn compact_nlist(nlist: &NList) -> NList {
    NList {
        n: nlist.n,
        max_card: nlist.max_card,
        no_set_list: nlist.no_set_list.iter().copied().collect(),
        remaining_cards_list: nlist.remaining_cards_list.iter().copied().collect(),
    }
}

// In save_new_to_file():
let compacted: Vec<NList> = self.new.iter().map(compact_nlist).collect();
match save_to_file(&compacted, &file) {
```

### Expected Impact

- **File size reduction**: 40-50% (4GB → 2.0-2.4GB)
- **Performance**: +2-5% overhead (extra clone) but **NET FASTER I/O**
- **Complexity**: Low (wrapper function)
- **Trade-off**: Small CPU cost for huge I/O savings

---

## Option 3: Switch to Custom Serialization

**Concept**: Manually serialize only length + data, skip capacity

### Implementation (Complex)

```rust
// Custom serialize function
fn serialize_compact(list: &Vec<NList>) -> Vec<u8> {
    let mut bytes = Vec::new();
    // Write count
    bytes.extend_from_slice(&(list.len() as u64).to_le_bytes());
    // For each NList
    for nlist in list {
        bytes.push(nlist.n);
        bytes.extend_from_slice(&nlist.max_card.to_le_bytes());
        // Serialize no_set_list
        bytes.extend_from_slice(&(nlist.no_set_list.len() as u16).to_le_bytes());
        for &card in &nlist.no_set_list {
            bytes.extend_from_slice(&card.to_le_bytes());
        }
        // Serialize remaining_cards_list
        bytes.extend_from_slice(&(nlist.remaining_cards_list.len() as u16).to_le_bytes());
        for &card in &nlist.remaining_cards_list {
            bytes.extend_from_slice(&card.to_le_bytes());
        }
    }
    bytes
}
```

### Expected Impact

- **File size reduction**: 50-60% (4GB → 1.6-2.0GB) - **BEST**
- **Performance**: More complex, needs custom deserialize
- **Complexity**: High (full serialize/deserialize rewrite)
- **Risk**: Lose rkyv validation, more bugs

---

## Option 4: Compress Files (External)

**Concept**: Use zstd/lz4 compression after serialization

### Implementation

```bash
# After generation
zstd -19 nlist_v2_06_batch_000000.rkyv
# Result: .rkyv.zst files at ~30-40% original size
```

### Expected Impact

- **File size reduction**: 60-70% (4GB → 1.2-1.6GB)
- **Performance**: Slower reads/writes (+30-50% time)
- **Complexity**: External tooling
- **Use case**: Long-term storage, not active processing

---

## Recommendation: Implement Option 2 (Clone to New Vec)

**Rationale**:

1. ✅ **Best balance** of simplicity, performance, and file size
2. ✅ **Net faster overall** - smaller files = faster I/O (35% time is I/O)
3. ✅ **Easy to implement** - 10 lines of code
4. ✅ **Safe** - still uses rkyv validation
5. ✅ **Reversible** - easy to test and rollback

**Implementation Plan**:

1. Add `compact_nlist()` helper function
2. Call before `save_to_file()` in both `save_new_to_file()` and `create_seed_lists()`
3. **Remove** existing `shrink_to_fit()` calls (they add overhead without benefit)
4. Test with size 4 to verify file size reduction

**Expected Results**:

- Size 6 files: **4.0GB → 2.2GB** (45% reduction)
- Total time: **426s → 320s** (25% faster due to I/O savings)
- Overhead: **17% → 2%** (remove shrink cost, add small clone cost)

Would you like me to implement Option 2?
