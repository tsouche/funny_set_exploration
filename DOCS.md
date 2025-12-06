# Documentation Index

Welcome to the funny_set_exploration project documentation!

## Quick Links

### Getting Started

- **[README.md](README.md)** - Start here! Project overview, quick start guide, and basic usage
- **[PATH_CONFIGURATION.md](PATH_CONFIGURATION.md)** - Configure custom output directories (NAS, external drives, etc.)

### Project History

- **[CHANGELOG.md](CHANGELOG.md)** - Complete version history and feature additions

### Technical Details

- **[TECHNICAL.md](TECHNICAL.md)** - In-depth technical documentation, architecture, and algorithms

### Examples

- **[examples/path_examples.rs](examples/path_examples.rs)** - Code examples for different path configurations

## What This Project Does

Exhaustively finds all combinations of Set cards (12, 15, and 18 cards) that contain no valid sets.

## Current Status (v0.2.2)

✅ **Working:**

- Complete algorithm implementation
- Batch file processing (20M lists per ~4GB file)
- Configurable output directories
- Memory-efficient processing

🔍 **In Progress:**

- Computing 7+ card combinations
- Performance optimization research

## Key Documentation by Task

### I want to...

**...run the program**
→ See [README.md - Quick Start](README.md#quick-start)

**...save files to my NAS drive**
→ See [PATH_CONFIGURATION.md](PATH_CONFIGURATION.md)

**...understand the algorithm**
→ See [README.md - Algorithm Principles](README.md#principle-of-the-algorithm)

**...understand the code structure**
→ See [TECHNICAL.md - Architecture](TECHNICAL.md#architecture-overview)

**...optimize performance**
→ See [TECHNICAL.md - Performance Metrics](TECHNICAL.md#performance-metrics)

**...see what changed in each version**
→ See [CHANGELOG.md](CHANGELOG.md)

**...contribute to the project**
→ See [README.md - Contributing](README.md#contributing)

## File Structure

```
funny_set_exploration/
├── README.md                    # Project overview and quick start
├── CHANGELOG.md                 # Version history
├── TECHNICAL.md                 # Technical documentation
├── PATH_CONFIGURATION.md        # Output directory configuration
├── DOCS.md                      # This file (documentation index)
├── Cargo.toml                   # Rust dependencies
├── src/
│   ├── main.rs                  # Entry point and configuration
│   ├── set.rs                   # Set game logic
│   ├── nlist.rs                 # N-list data structure
│   ├── list_of_nlists.rs        # Batch processing and file I/O
│   └── utils.rs                 # Utility functions
└── examples/
    └── path_examples.rs         # Path configuration examples
```

## Output Files

Generated files follow this naming pattern:
```
nlist_{size:02}_batch_{number:03}.bin
```

Examples:

- `nlist_03_batch_000.bin` - First batch of 3-card lists
- `nlist_07_batch_042.bin` - 43rd batch of 7-card lists

Each file is approximately 4GB and contains up to 20 million n-lists.

## Quick Facts

| Metric | Value |
|--------|-------|
| Language | Rust (2024 edition) |
| Current Version | 0.2.2 |
| RAM Required | ~13.5GB peak |
| File Size | ~4GB per batch |
| Batch Size | 20M n-lists (configurable) |
| Dependencies | serde, bincode, separator |

## Completed Computations

| Size | Count | Status |
|------|-------|--------|
| 3-card | 58,896 | ✅ Complete |
| 4-card | 1,098,240 | ✅ Complete |
| 5-card | 13,394,538 | ✅ Complete |
| 6-card | 155,769,345 | ✅ Complete |
| 7-card | TBD | 🔄 In Progress |

## Future Enhancements

See [CHANGELOG.md - Future Considerations](CHANGELOG.md#future-considerations) for:

- rkyv serialization migration (zero-copy, faster)
- Parallel processing
- GPU acceleration
- Checkpoint/resume capability
- Compression options

## Support

For questions or issues:

1. Check the relevant documentation above
2. Review [TECHNICAL.md - Known Issues](TECHNICAL.md#known-issues)
3. Check git history for similar problems
4. Open an issue on GitHub

## License

See LICENSE file for details.

---

**Last Updated:** December 6, 2025  
**Version:** 0.2.2
