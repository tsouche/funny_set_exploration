/// Manage the search for the grail of Set: combinations of 12 / 15 / 18 cards 
/// with no sets
///
/// Version 0.3.2 - Hybrid: Stack computation + Heap I/O
/// 
/// CLI Usage:
///   funny.exe --size 5 -o T:\data\funny_set_exploration  # Build size 5 from size 4
///   funny.exe --size 5-7 -o T:\data\funny_set_exploration # Build sizes 5, 6, and 7
///   funny.exe                                             # Default mode (sizes 4-6)
///
/// Arguments:
///   --size, -s <SIZE>       Target size to build (4-18, or range like 5-7)
///                           If omitted, runs default behavior (creates seeds + sizes 4-6)
///   --output-path, -o       Optional: Directory for output files
///                           Defaults to current directory
///
/// Implementation:
///   - Hybrid approach: NoSetList (stack) for fast computation, NoSetListSerialized (heap) for compact I/O
///   - Creates .rkyv files with size_32 encoding (~2GB per 20M batch)
///   - 4-5× faster than heap-only v0.2.2 while maintaining compact file sizes

mod utils;
mod set;
mod no_set_list;
mod list_of_nlists;

use clap::Parser;
use separator::Separatable;
use crate::utils::*;

/// CLI arguments structure
#[derive(Parser, Debug)]
#[command(name = "funny_set_exploration")]
#[command(about = "Generate no-set lists for the Set card game", long_about = None)]
struct Args {
    /// Target size for the no-set lists (4-18 or range like 5-7)
    /// 
    /// If not provided, runs the default behavior (creates seeds + sizes 4-6)
    /// - Single size: "5" builds size 5 from size 4 files
    /// - Range: "5-7" builds sizes 5, 6, and 7 sequentially
    /// - Size 4: Builds from seed lists (size 3)
    /// - Size 5+: Requires files from previous size
    #[arg(short, long)]
    size: Option<String>,

    /// Output directory path (optional)
    /// 
    /// Examples:
    ///   Windows: T:\data\funny_set_exploration
    ///   Linux:   /mnt/nas/data/funny_set_exploration
    ///   Relative: ./output
    #[arg(short, long)]
    output_path: Option<String>,
}

/// Parse size argument into start and end range
/// Examples: "5" -> (5, 5), "5-7" -> (5, 7)
fn parse_size_range(size_str: &str) -> Result<(u8, u8), String> {
    if size_str.contains('-') {
        let parts: Vec<&str> = size_str.split('-').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid range format: '{}'. Expected format: '5-7'", size_str));
        }
        let start: u8 = parts[0].trim().parse()
            .map_err(|_| format!("Invalid start size: '{}'", parts[0]))?;
        if start < 4 || start > 18 {
            return Err(format!("Start size {} out of range (4-18)", start));
        }
        let end: u8 = parts[1].trim().parse()
            .map_err(|_| format!("Invalid end size: '{}'", parts[1]))?;
        if end < 4 || end > 18 {
            return Err(format!("End size {} out of range (4-18)", end));
        }
        if start > end {
            return Err(format!("Start size {} cannot be greater than end size {}", start, end));
        }
        Ok((start, end))
    } else {
        let size: u8 = size_str.trim().parse()
            .map_err(|_| format!("Invalid size: '{}'", size_str))?;
        if size < 4 || size > 18 {
            return Err(format!("Size {} out of range (4-18)", size));
        }
        Ok((size, size))
    }
}

fn main() {
    // Parse command-line arguments
    let args = Args::parse();

    /// Max number of n-list saved per file for v0.3.2
    /// - Each NoSetList: 792 bytes during compute (stack)
    /// - Each NoSetListSerialized: ~100 bytes after conversion (heap)
    /// - 20M entries × 100 bytes = ~2GB per file after serialization
    /// - Peak RAM during save: ~10.5GB (vec + archive + overhead)
    const MAX_NLISTS_PER_FILE: u64 = 20_000_000;

    // Initialize log file for test_print output
    init_log_file();

    debug_print_on();
    debug_print_off();
    test_print_off();
    test_print_on();

    // Parse size range if provided
    let size_range = if let Some(ref size_str) = args.size {
        match parse_size_range(size_str) {
            Ok(range) => Some(range),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    use crate::list_of_nlists::ListOfNlist;

    banner("Funny Set Exploration)");
    
    if let Some((start_size, end_size)) = size_range {
        // =====================================================================
        // CLI MODE: Process size range
        // =====================================================================
        if start_size == end_size {
            test_print(&format!("Target size = {} cards", start_size));
        } else {
            test_print(&format!("Size range = {} to {} cards", start_size, end_size));
        }
        test_print("Strategy: Stack computation + Heap I/O");
        test_print(&format!("Batch size: {} entries/file (~2GB, compact)", MAX_NLISTS_PER_FILE.separated_string()));
        
        if let Some(ref path) = args.output_path {
            test_print(&format!("Output directory: {}", path));
        } else {
            test_print("Output directory: current directory");
        }
        test_print("\n======================\n");

        // Initialize ListOfNlist with optional custom path
        let mut no_set_lists: ListOfNlist = match args.output_path {
            Some(path) => ListOfNlist::with_path(&path),
            None => ListOfNlist::new(),
        };

        // Handle size 4: need to create seed lists first
        if start_size == 4 {
            test_print("Creating seed lists (size 3)...");
            no_set_lists.create_seed_lists();
            test_print("Seed lists created successfully.\n");
        }

        // Process each size in the range
        for target_size in start_size..=end_size {
            let source_size = target_size - 1;
            test_print(&format!("Start processing files to create no-set-lists of size {}:", target_size));
            
            let _nb_new = no_set_lists.process_all_files_of_current_size_n(
                source_size, 
                &MAX_NLISTS_PER_FILE
            );
            
            test_print(&format!("\nCompleted size {}! Generated files: nlist_{:02}_batch_*.rkyv\n", 
                target_size, target_size));
        }
    } else {
        // =====================================================================
        // DEFAULT MODE - process the whole pipeline: seeds + sizes 4 to 18
        // =====================================================================
        test_print("   - will create          58.896 no-set-lists with  3 cards");
        test_print("   - will create       1.004.589 no-set-lists with  4 cards");
        test_print("   - will create      13.394.538 no-set-lists with  5 cards");
        test_print("   - will create     141.370.218 no-set-lists with  6 cards");
        test_print("   - will create   1.180.345.041 no-set-lists with  7 cards");
        test_print("   - will create   7.920.450.378 no-set-lists with  8 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with  9 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with 10 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with 11 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with 12 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with 13 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with 14 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with 15 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with 16 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with 17 cards");
        test_print("   - will create  __.___.___.___ no-set-lists with 18 cards");
        test_print("\n======================\n");

        // Initialize with output path if provided
        let mut no_set_lists: ListOfNlist = match args.output_path {
            Some(path) => ListOfNlist::with_path(&path),
            None => ListOfNlist::with_path(r"T:\data\funny_set_exploration"),
        };

        // Create all seed lists
        test_print("Creating seed lists...");
        no_set_lists.create_seed_lists();

        // Expand from seed_lists to size 4, 5, 6...
        for size in 3..17 {
            test_print(&format!("\nStart processing files to create no-set-lists of size {}:", size+1));
            let _nb_new = no_set_lists.process_all_files_of_current_size_n(size, 
                &MAX_NLISTS_PER_FILE);
        }
    }
}
