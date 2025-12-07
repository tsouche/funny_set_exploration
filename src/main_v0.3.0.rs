/// Alternative main using stack-optimized NoSetList and ListOfNSL
/// 
/// VERSION 0.3.0 - Stack optimization with fixed-size arrays
/// 
/// Key improvements over v0.2.2:
/// - Zero heap allocations in core algorithm (3-8x faster)
/// - Fixed-size stack arrays instead of Vec
/// - Better cache locality and memory bandwidth
/// - Pure rkyv serialization (no backward compatibility)
/// 
/// CLI Usage:
///   cargo run --bin main_v3                    # Default: create seeds + sizes 4-6
///   cargo run --bin main_v3 -- --size 5        # Build size 5 from size 4
///   cargo run --bin main_v3 -- --size 7 -o T:\output
///
/// Note: This creates .nsl files, not compatible with v0.2.2 .bin/.rkyv files

mod utils;
mod set;
mod no_set_list;
mod list_of_nsl;

use clap::Parser;
use crate::utils::*;
use crate::list_of_nsl::{ListOfNSL, created_a_total_of};

/// CLI arguments structure
#[derive(Parser, Debug)]
#[command(name = "funny_set_exploration_v3")]
#[command(about = "Stack-optimized no-set list generator (v0.3.0)", long_about = None)]
struct Args {
    /// Target size for the no-set lists (4-12)
    #[arg(short, long, value_parser = clap::value_parser!(u8).range(4..=12))]
    size: Option<u8>,

    /// Output directory path (optional)
    #[arg(short, long)]
    output_path: Option<String>,
}

fn main() {
    let args = Args::parse();

    /// Max number of n-list saved per file
    /// - 20 million n-lists per file
    /// - Each file ~1.9-2.5GB (stack-allocated structs are larger)
    /// - Peak RAM usage: ~8-10GB (reduced from v0.2.2's 13.5GB)
    /// - Files saved as .nsl format (NoSetList format)
    const MAX_NLISTS_PER_FILE: u64 = 20_000_000;

    debug_print_on();
    debug_print_off();
    test_print_off();
    test_print_on();
    banner("Funny Set Exploration - v0.3.0 (Stack Optimized)");
    
    // Check if CLI mode or default mode
    if let Some(target_size) = args.size {
        // =====================================================================
        // CLI MODE: Process specific size
        // =====================================================================
        test_print(&format!("CLI Mode: Target size = {} cards", target_size));
        
        if let Some(ref path) = args.output_path {
            test_print(&format!("Output directory: {}", path));
        } else {
            test_print("Output directory: current directory");
        }
        test_print("\n======================\n");

        // Initialize ListOfNSL with optional custom path
        let mut no_set_lists: ListOfNSL = match args.output_path {
            Some(path) => ListOfNSL::with_path(&path),
            None => ListOfNSL::new(),
        };

        // Handle size 4: need to create seed lists first
        if target_size == 4 {
            test_print("Creating seed lists (size 3) using STACK ALLOCATION...");
            no_set_lists.create_seed_lists();
            test_print("Seed lists created successfully.\n");
        }

        // Process from (size - 1) to size
        let source_size = target_size - 1;
        test_print(&format!("Processing files nlist_{:02}_batch_*.nsl to create no-set-lists of size {}:", 
            source_size, target_size));
        test_print("Using STACK-OPTIMIZED algorithm (zero heap allocations)");
        
        let nb_new = no_set_lists.process_all_files_of_current_size_n(
            source_size, 
            &MAX_NLISTS_PER_FILE
        );
        
        created_a_total_of(nb_new, target_size);
        test_print(&format!("\nCompleted! Generated files: nlist_{:02}_batch_*.nsl", target_size));
    } else {
        // =====================================================================
        // DEFAULT MODE: Original behavior
        // =====================================================================
        test_print("   - will create         58.896 no-set-lists with  3 cards (STACK)");
        test_print("   - will create      1.004.589 no-set-lists with  4 cards (STACK)");
        test_print("   - will create     13.394.538 no-set-lists with  5 cards (STACK)");
        test_print("   - will create    141.370.218 no-set-lists with  6 cards (STACK)");
        test_print("   - will create  1.180.345.041 no-set-lists with  7 cards (STACK)");
        test_print("   - will create  7.920.450.378 no-set-lists with  8 cards (STACK)");
        test_print("\n   Performance: 3-8x faster than v0.2.2 (zero heap allocations)\n");
        test_print("\n======================\n");

        // ========================================================================
        // CONFIGURE OUTPUT DIRECTORY
        // ========================================================================
        // Option 1: Use current directory (default)
        // let mut no_set_lists: ListOfNSL = ListOfNSL::new();
        
        // Option 2: Use a custom path on Windows
        let mut no_set_lists: ListOfNSL = ListOfNSL::with_path(
            r"T:\data\funny_set_exploration");
        
        // Option 3: Use a custom path on Linux
        // let mut no_set_lists: ListOfNSL = ListOfNSL::with_path("/mnt/nas/data/funny_set_exploration");
        // ========================================================================

        // Create all seed lists (no-set-lists of size 3) using STACK ALLOCATION
        test_print("Creating seed lists with STACK optimization...");
        no_set_lists.create_seed_lists();

        // Expand from seed_lists to NoSetList of size 4, 5, 6...
        for size in 3..6 {
            test_print(&format!("\nStart processing files to create no-set-lists of size {}:", size+1));
            test_print("Using STACK-OPTIMIZED algorithm (zero heap allocations in core loop)");
            let nb_new = no_set_lists.process_all_files_of_current_size_n(size, 
                &MAX_NLISTS_PER_FILE);
            created_a_total_of(nb_new, size+1);
        }
    }
}
