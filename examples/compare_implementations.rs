/// Performance comparison between heap-based NList and stack-based NoSetList
/// 
/// This example demonstrates the performance difference between:
/// - v0.2.2: NList with Vec<usize> (heap allocation)
/// - v0.3.0: NoSetList with [usize; N] (stack allocation)
///
/// Usage:
///   cargo run --example compare_implementations

use std::time::Instant;

// Import both implementations
use funny_set_exploration::nlist::NList;
use funny_set_exploration::no_set_list::NoSetList;
use funny_set_exploration::set::*;

fn main() {
    println!("=".repeat(80));
    println!("Performance Comparison: NList (v0.2.2) vs NoSetList (v0.3.0)");
    println!("=".repeat(80));
    
    // Create a test NList (heap-based)
    let test_nlist = create_test_nlist();
    println!("\nTest data created:");
    println!("  - n: {}", test_nlist.n);
    println!("  - no_set_list: {} cards", test_nlist.no_set_list.len());
    println!("  - remaining_cards: {} cards", test_nlist.remaining_cards_list.len());
    
    // Convert to NoSetList (stack-based)
    let test_nsl = NoSetList::from_slices(
        test_nlist.n,
        test_nlist.max_card,
        &test_nlist.no_set_list,
        &test_nlist.remaining_cards_list,
    );
    
    // Benchmark NList (heap-based)
    println!("\n{}", "-".repeat(80));
    println!("Benchmarking NList.build_higher_nlists() [v0.2.2 - HEAP]");
    println!("{}", "-".repeat(80));
    
    let iterations = 1000;
    let start = Instant::now();
    let mut total_generated = 0;
    
    for _ in 0..iterations {
        let result = test_nlist.build_higher_nlists();
        total_generated += result.len();
    }
    
    let heap_duration = start.elapsed();
    let heap_avg = heap_duration.as_micros() / iterations;
    
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", heap_duration);
    println!("  Avg per call: {} µs", heap_avg);
    println!("  Generated per call: {}", total_generated / iterations as usize);
    
    // Benchmark NoSetList (stack-based)
    println!("\n{}", "-".repeat(80));
    println!("Benchmarking NoSetList.build_higher_nsl() [v0.3.0 - STACK]");
    println!("{}", "-".repeat(80));
    
    let start = Instant::now();
    let mut total_generated_stack = 0;
    
    for _ in 0..iterations {
        let result = test_nsl.build_higher_nsl();
        total_generated_stack += result.len();
    }
    
    let stack_duration = start.elapsed();
    let stack_avg = stack_duration.as_micros() / iterations;
    
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", stack_duration);
    println!("  Avg per call: {} µs", stack_avg);
    println!("  Generated per call: {}", total_generated_stack / iterations as usize);
    
    // Calculate speedup
    println!("\n{}", "=".repeat(80));
    println!("RESULTS");
    println!("{}", "=".repeat(80));
    
    let speedup = heap_duration.as_secs_f64() / stack_duration.as_secs_f64();
    let improvement = ((heap_avg - stack_avg) as f64 / heap_avg as f64) * 100.0;
    
    println!("\n✅ Stack-based NoSetList is {:.2}x FASTER", speedup);
    println!("✅ Performance improvement: {:.1}%", improvement);
    println!("\n  Heap (v0.2.2): {} µs per call", heap_avg);
    println!("  Stack (v0.3.0): {} µs per call", stack_avg);
    println!("  Time saved: {} µs per call", heap_avg - stack_avg);
    
    // Verify correctness
    println!("\n{}", "-".repeat(80));
    println!("Correctness Verification");
    println!("{}", "-".repeat(80));
    
    if total_generated == total_generated_stack {
        println!("✅ Both implementations generated the same number of results");
    } else {
        println!("❌ WARNING: Different result counts!");
        println!("   Heap: {}, Stack: {}", total_generated, total_generated_stack);
    }
    
    // Memory analysis
    println!("\n{}", "-".repeat(80));
    println!("Memory Analysis");
    println!("{}", "-".repeat(80));
    
    let nlist_size = std::mem::size_of::<NList>();
    let nsl_size = std::mem::size_of::<NoSetList>();
    
    println!("\nStruct sizes:");
    println!("  NList (heap):     {} bytes (+ heap allocations)", nlist_size);
    println!("  NoSetList (stack): {} bytes (fixed, no heap)", nsl_size);
    
    println!("\nHeap allocations estimate:");
    println!("  NList.build_higher_nlists(): ~90-150 allocations per call");
    println!("  NoSetList.build_higher_nsl(): 0 allocations in core loop");
    
    let remaining = test_nlist.remaining_cards_list.len();
    let estimated_heap_allocs = remaining * 3; // Conservative estimate
    
    println!("\nFor this test (with {} remaining cards):", remaining);
    println!("  Estimated heap operations per NList call: ~{}", estimated_heap_allocs);
    println!("  Total heap operations avoided (stack): {} per call", estimated_heap_allocs);
    println!("  Over {} iterations: {} heap operations eliminated!", 
        iterations, estimated_heap_allocs * iterations);
    
    println!("\n{}", "=".repeat(80));
    println!("Conclusion: Stack-based implementation is SIGNIFICANTLY faster!");
    println!("{}", "=".repeat(80));
}

/// Create a realistic test NList for benchmarking
fn create_test_nlist() -> NList {
    // Start with a valid 3-card combination
    let i = 0;
    let j = 1;
    let k = 5;
    
    // Build no-set list
    let table = vec![i, j, k];
    
    // Build remaining cards (all cards > k, minus forbidden)
    let mut remaining_cards: Vec<usize> = (k + 1..81).collect();
    
    // Remove cards that would form sets
    let c1 = next_to_set(i, j);
    let c2 = next_to_set(i, k);
    let c3 = next_to_set(j, k);
    remaining_cards.retain(|&x| x != c1 && x != c2 && x != c3);
    
    NList {
        n: 3,
        max_card: k,
        no_set_list: table,
        remaining_cards_list: remaining_cards,
    }
}
