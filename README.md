# Funny Set Exploration

A Rust-based exhaustive search algorithm to find all combinations of 12, 15, and 18 Set cards with no valid sets.

## Project Status

**Current Version:** 0.4.8 (December 2025)

**Key Features:**

- **GlobalFileState**: In-memory state with atomic JSON/TXT persistence for O(1) file lookups
- **Unified --size mode**: Single mode for both normal processing and restart from batch
- **Incremental state saving**: JSON/TXT files updated after each output file is created
- Dual input/output path support for safer processing
- Repository integrity checking (missing batches/files detection)
- 6-digit batch numbering for scalability
- Continuous batch numbering across source files

**Architecture:** Hybrid stack/heap implementation

- **NoSetList** (stack arrays): Zero-allocation computation, 4-5× faster
- **NoSetListSerialized** (heap Vec): Compact 2GB files with rkyv size_32
- **Best of both worlds**: Fast computation + compact storage

**Working Features:**

- ✅ Hybrid implementation (stack compute + heap I/O)
- ✅ Zero-copy serialization with rkyv (10-100x faster reads)
- ✅ Memory-mapped file I/O for optimal performance
- ✅ Unified CLI with --size mode (supports restart from batch)
- ✅ Extended size support (3-18 cards)
- ✅ Batch file processing (20M entries per file, ~2GB each)
- ✅ Configurable output directories (local, network, NAS support)
- ✅ Progress tracking and formatted output

**Completed Computations:**

- 3-card lists: 58,896 combinations
- 4-card lists: 1,004,589 combinations
- 5-card lists: 14,399,538 combinations
- 6-card lists: 155,769,345 combinations
- 7-card lists: 1,180,345,041 combinations
- 8-card lists: In progress...

## Quick Start

### Prerequisites

- Rust toolchain (2024 edition or later)
- ~12-15GB RAM for processing
- Significant disk space for output files (~2GB per batch file)

### Build and Run

```bash
# Clone the repository
git clone https://github.com/tsouche/funny_set_exploration.git
cd funny_set_exploration

# Build the project
cargo build --release

# Run with default behavior (sizes 4-6)
./target/release/funny_set_exploration

# Run with specific size
./target/release/funny_set_exploration --size 7

# Run with custom output directory
./target/release/funny_set_exploration --size 7 --output-path T:\data\funny_set_exploration

# Restart from specific input batch
./target/release/funny_set_exploration --size 5 2 -i ./input -o ./output

# Process single batch only (unitary mode - canonical way to fix defective files)
./target/release/funny_set_exploration --unitary 5 2 -o T:\data\funny_set_exploration

# Count existing files
./target/release/funny_set_exploration --count 6 -o T:\data\funny_set_exploration

# Check repository integrity
./target/release/funny_set_exploration --check 6 -o T:\data\funny_set_exploration

# Compact small files into larger batches
./target/release/funny_set_exploration --compact 8 -o T:\data\funny_set_exploration
```

### CLI Options

```bash
funny_set_exploration [OPTIONS]

Options:
  -s, --size <SIZE> [BATCH]
          Target output size to build (3-18), optional batch to restart from
          If omitted, runs default behavior (creates seeds + sizes 4-18)
          With BATCH: restart from specific input batch number

      --unitary <SIZE> <BATCH>
          Process only one specific input batch (unitary processing)
          This is the ONLY canonical way to overwrite/fix defective files.
          SIZE refers to INPUT size. Reprocesses only this batch.
          Use --force to regenerate count file first.

      --count <SIZE>
          Count existing files for target size, create summary report
          Creates nsl_{output_size:02}_global_count.txt without processing new lists

      --check <SIZE>
          Check repository integrity for target size
          Validates batch sequence, consolidated count file, and intermediary files
          Reports missing batches and files with [OK]/[!!] indicators

      --compact <SIZE>
          Compact small output files into larger 10M-entry batches
          SIZE refers to OUTPUT size. Consolidates files and writes new compacted files (originals preserved).
          New format: nsl_{size}c_batch_{batch}_from_{first_source}.rkyv (note the 'c' suffix after the size)
          Use when later processing waves create many small files.
          Intermediary count files (input-intermediary) now use the pattern:
            nsl_{output_size:02}_intermediate_count_from_{input_size:02}_{input_batch:06}.txt
          Use the script `scripts/rename_intermediary_counts.ps1` to migrate existing intermediary files to the new naming scheme.

      --force
          Force regeneration of count file (use with --count, --size BATCH, or --unitary)

  -o, --output-path <OUTPUT_PATH>
          Output directory path (optional)
          Examples:
            Windows: T:\data\funny_set_exploration
            Linux:   /mnt/nas/data/funny_set_exploration
            Relative: ./output

  -h, --help
          Print help
```

### Output Files

Files are named: `nlist_{size:02}_batch_{batch:06}.rkyv`

Examples:

- `nlist_05_batch_000000.rkyv` - First batch of 5-card lists (~1GB)
- `nlist_06_batch_000000.rkyv` - First batch of 6-card lists (~1GB)
- `nlist_07_batch_000059.rkyv` - 60th batch of 7-card lists

## Architecture

### Version History

- **v0.4.9** (Current): Refactored compaction to use GlobalFileState, multi-file crash-safe compaction
- **v0.4.8**: GlobalFileState with incremental JSON/TXT persistence, unified --size mode
- **v0.4.7**: 6-digit batch formatting, input-intermediary tracking
- **v0.4.6**: Added input-intermediary file generation and atomic writes
- **v0.4.1**: Renamed modes for clarity, improved count file format
- **v0.4.0**: Added restart capability with modular file naming
- **v0.3.2**: Simplified to hybrid-only implementation
  - Removed v0.2.2 (heap-based) and v0.3.0 (stack-only)
  - Cleaner codebase with single optimized approach
  - NoSetListSerialized for clear separation of concerns

- **v0.3.1**: Hybrid stack/heap approach
  - Combined stack computation (fast) with heap I/O (compact)
  - 23% faster overall than v0.2.2
  - Same 2GB file size as v0.2.2

- **v0.3.0**: Full stack implementation
  - 6-7× faster computation
  - 7-8× larger files (15GB) - I/O bottleneck
  - Proved stack optimization benefits

- **v0.2.2**: Original heap-based with rkyv
  - Baseline implementation
  - 2GB files with zero-copy deserialization

### Performance Comparison

**Size 6 processing (155M lists):**

| Metric | v0.4.1 (Current) | v0.2.2 (Heap) |
|--------|------------------|---------------|
| Total time | ~308s | ~398s |
| Computation | ~19s (6%) | ~225s (56%) |
| File I/O | ~166s (54%) | ~140s (35%) |
| File size | ~2GB | ~2GB |

**Key improvements:**

- 10× faster core algorithm (stack operations)
- Same compact file format (rkyv with Vec)
- 23% faster overall despite conversion overhead

## Principle of the algorithm

There are 81 cards in Set: the set cards are represented with a u8 of value 0 to 80 (included): this is enough to fully represent the cards. A set is considered valid if... (see the Set Game repositories). A table will always contain a multiple of 3 cards (_3n_ cards). Our purpose is to identify the **exhaustive** list of combination of 12, 15 and 18 cards which do NOT include any valid set. To do so we crawl all the possible combinations of 12 / 15 / 18 cards and test the presence or absence of a set.

Due to the very large number of combination of 12/15/18 cards amongst 81, it is critical to optimize the seach algorithm to be able to finish the search in a 'decent' timeframe.

- The first critical efficiency criteria is that the order of the cards does not matter when it comes to identifying a set on a table: only the values matter. So when we crawl the graph of possibilities, we do not look'backward' at the cards (i.e. since we will crawl the possibilities in increasing card value, we will not look at combinations with cards below the value of one of the card already on the table... sice such cards have already been looked at).
- The second critical efficiceny criteria is that it is cheap to compute the third card which will complement a given tuple of 2 cards to form a valid set: it is actually much cheaper than parsing all possiblilites and compute wheter all possible tripplets form a valid set.

So, considering a given list of N cards (with values from 0 to MAX):

- we create the complementary list of all 'remaining cards' (i.e. all the cards of value above MAX)
- we list all the cards which we know form a valid set with 2 cards in the list (and we store these in the list of 'forbidden' cards)
- by deduction, we can build the list of 'possible' complementary cards (all the cards between MAX+1 and 80 which are not in the 'forbidden list') which could extend the list to create a new list of N+1 cards.

Thus, increasing gradually the number of cards in the list, we reach N = 12, and then continue to N = 15 and eventually to N = 18.

## Proposed implementation

 1. create the list seeds
 2. expand the lists from level n-1 to n until n = 18
 3. store results for n = 12, 15 and 18

### What is a list seed?

A list seed is a triplet of 3 cards which do not form a valid set. This is the minimal length of list we consider (since one need at least 3 cards to form a valid set):

- we build such a list of 3 cards, of values up to MAX:
- for any couple of card in this list, we compute the value of the 'third' card which would form a valid set with the considered couple:
  - if the value is below MAX: it was already discarded
  - if the value is above MAX: we mark this value as to be discarded in any future search
- at the end of this pass, we have a list of 3 cards, and a list of 'remaining card' which we know does not contain any card which would form a set if it were added to the list.

This combination of two lists (3 cards not forming a set, and the corresponding 'remaingin list') is a 'list seed'.

## How do we grow a list?

We start from a 'seed list' which we call a '03-list' (i.e. a list of 3 cards and the corresponding 'remaining list').

Lest' describe how - from a valid '03-list' - we will build all the possible valid '04-list', i.e. a list of 4 cards within which we cannot find any combination of 3 cards which form valid set, and the corresponding list of 'remaining cards' which do not form a valid set with any of the cards in the list of 4 cards.
More generically, let's decribe how - from a valid 'n-list' - we will build the list of all possible 'n+1-lists', with the following definition:

  A 'n-list' is a couple of lists:
      - a 'primary list' of n cards (with 3 =< n =< 18, of values =< MAX), within which we can't find any combination of 3 cards forming a valid set
      - with a list of 'remaining cards' which contains **all** the cards of value > MAX, which will not form a valid set with any couple of cards from the 'primary list'

Assuming that the 'n-list' is valid, here is how we build the list of all possible 'n+1-list':

- for all card _C* in the remaining list:
  - create a 'n+1-primary list' with the existing 'primary list' extended with _C_
  - create a 'cadidate n+1-remaining list' for the 'primary list + _C'_:
    - start from the 'remaining card' list
    - discard any card in this remaining list of a value =< _C_ : this becomes the 'candidate n+1-remaining list'
    - for any card _P_ in the 'primary list':
      - compute the thid card _D_ which form a valid set with _C_ and _P_
      - check if _D_ is in the 'candidate n+1-remaining list': if yes, remove it from the list
    - if there are not enough cards left in the 'candidate n+1-remaining list' to complement the 'primary list' to 12 cards, it means that the card C is a dead-end: drop it and go to the next card _C_
    - else you have created a valid n+1-list: store it for later processing, and move the next card _C_

Thus, from the exhaustive set of 03-lists, you create the exhaustive st of 04-lists... and so on until you reach 12 cards.

From the 12-lists, you can build teh 12-, 14- and 15-lists.

Form the 15-lists, you can build the 16-, 17- and 18-list.

We know that any able with 21 card will count multiple valid sets, so it is not usefull to ge beyond 18 cards.
We could however compute - for the fun - the list of all possible 19- and 20-lists if there are any.

## Implementation Details

### File Organization

The project is organized into the following modules:

- **`src/main.rs`**: Entry point, configuration, and main processing loop
- **`src/set.rs`**: Set game logic and card validation functions
- **`src/nlist.rs`**: NList structure representing n-card combinations
- **`src/list_of_nlists.rs`**: Batch processing, file I/O, and n+1 list generation
- **`src/utils.rs`**: Debug printing and utility functions

### Data Structures

**NList**: Represents a combination of n cards with no sets

```rust
pub struct NList {
    pub n: u8,                           // Number of cards
    pub max_card: usize,                 // Highest card value
    pub no_set_list: Vec<usize>,         // Cards in the combination
    pub remaining_cards_list: Vec<usize> // Valid cards that can be added
}
```

**ListOfNlist**: Manages batch processing and file operations

```rust
pub struct ListOfNlist {
    pub current_size: u8,          // Size of current n-lists
    pub current: Vec<NList>,       // Current n-lists being processed
    pub new: Vec<NList>,           // Newly generated n+1-lists
    pub base_path: String,         // Output directory for files
    // ... tracking fields
}
```

### File Format

Output files use rkyv zero-copy serialization with modular naming; batch fields are 6-digit zero-padded:

```bash
nsl_{source_size:02}_batch_{source_batch:06}_to_{target_size:02}_batch_{target_batch:06}.rkyv
```

Examples:

- `nsl_03_batch_000000_to_04_batch_000000.rkyv` - From size 3 batch 0 to size 4 batch 0
- `nsl_05_batch_000001_to_06_batch_000012.rkyv` - From size 5 batch 1 to size 6 batch 12
- `nsl_10_batch_000000_to_11_batch_000000.rkyv` - Size 11 target uses 6-digit batch numbers

### Performance Characteristics

**Memory Usage:**

- Peak: ~13.5GB when batch is being saved
- Baseline: ~5GB after save completes
- Scales with `MAX_NLISTS_PER_FILE` setting (default: 20 million)

**File Sizes:**

- ~4GB per batch file at default settings
- ~8 batches for 6-card lists = ~32GB total

**Processing Speed:**

- Depends on number of valid combinations
- 3-card lists: Nearly instant
- 6-card lists: Minutes to hours
- 7+ card lists: Hours to days per size increment

## Configuration

### Adjusting Batch Size

In `src/main.rs`, modify `MAX_NLISTS_PER_FILE`:

```rust
const MAX_NLISTS_PER_FILE: u64 = 20_000_000;  // Default: 20 million
```

**Trade-offs:**

- Larger: Fewer files, more RAM usage, longer save times
- Smaller: More files, less RAM usage, more frequent disk I/O

### Setting Output Directory

See [`PATH_CONFIGURATION.md`](PATH_CONFIGURATION.md) for complete guide.

**Quick examples:**

```rust
// Default - current directory
let mut no_set_lists = ListOfNlist::new();

// Windows NAS drive
let mut no_set_lists = ListOfNlist::with_path(r"T:\data\funny_set_exploration");

// Linux NAS mount
let mut no_set_lists = ListOfNlist::with_path("/mnt/nas/data/funny_set_exploration");
```

## Dependencies

- **rkyv** (0.7): Zero-copy serialization framework
- **memmap2**: Memory-mapped file I/O
- **separator** (0.4): Number formatting with thousands separators
- **clap** (4.5): CLI argument parsing
- **chrono**: Timestamp generation for count files

## Future Optimizations

### Planned Improvements

**1. Serialization Upgrade (rkyv)**

- Zero-copy deserialization (10-100x faster reads)
- Reduced memory usage (~4-5GB vs current ~13.5GB peak)
- Memory-mapped file support
- See analysis in documentation for migration details

**2. Symmetry Reduction**

- Exploit card rotation properties to reduce search space
- Potential 4x reduction (color rotations)
- Investigate multi-attribute rotations (16x or more)

**3. Parallel Processing**

- Multi-threaded batch processing
- Independent n-list expansion can be parallelized
- GPU acceleration for set validation

**4. Enhanced Features**

- Checkpoint/resume capability for long-running computations
- Compressed storage formats
- Analysis and visualization tools
- Statistics and distribution analysis of results

## Contributing

This project is part of an exploration of the Set card game mathematics. Contributions, optimizations, and ideas are welcome!

## License

See LICENSE file for details.

## Related Projects

- [Set Game Rules](https://en.wikipedia.org/wiki/Set_(card_game))
- Other Set exploration repositories by @tsouche

## Acknowledgments

This exploration builds on the mathematical properties of the Set game and aims to exhaustively catalog all maximum no-set combinations.

---

**For detailed documentation:**

- See [`CHANGELOG.md`](CHANGELOG.md) for version history
- See [`PATH_CONFIGURATION.md`](PATH_CONFIGURATION.md) for output directory configuration
- See source code comments for implementation details
