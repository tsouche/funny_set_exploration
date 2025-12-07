/// Test to measure actual vs expected rkyv serialization size

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[derive(Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
struct TestNList {
    n: u8,
    max_card: usize,
    no_set_list: Vec<usize>,
    remaining_cards_list: Vec<usize>,
}

fn main() {
    // Create a test NList for size 6
    let mut test_list = TestNList {
        n: 6,
        max_card: 50,
        no_set_list: vec![1, 5, 10, 20, 30, 50],  // 6 cards
        remaining_cards_list: (51..81).collect(), // 30 remaining
    };
    
    // Test 1: Serialize as-is
    let bytes1 = rkyv::to_bytes::<_, 256>(&test_list).unwrap();
    println!("Test 1 - Normal Vec: {} bytes", bytes1.len());
    
    // Test 2: After shrink_to_fit
    test_list.no_set_list.shrink_to_fit();
    test_list.remaining_cards_list.shrink_to_fit();
    let bytes2 = rkyv::to_bytes::<_, 256>(&test_list).unwrap();
    println!("Test 2 - After shrink: {} bytes", bytes2.len());
    
    // Test 3: Fresh clone
    let compacted = TestNList {
        n: test_list.n,
        max_card: test_list.max_card,
        no_set_list: test_list.no_set_list.iter().copied().collect(),
        remaining_cards_list: test_list.remaining_cards_list.iter().copied().collect(),
    };
    let bytes3 = rkyv::to_bytes::<_, 256>(&compacted).unwrap();
    println!("Test 3 - Fresh clone: {} bytes", bytes3.len());
    
    // Expected size calculation
    println!("\nExpected size breakdown:");
    println!("  n (u8): 1 byte");
    println!("  max_card (usize): 8 bytes");
    println!("  no_set_list header: ~16 bytes (len + cap + ptr)");
    println!("  no_set_list data: 6 * 8 = 48 bytes");
    println!("  remaining_cards_list header: ~16 bytes");
    println!("  remaining_cards_list data: 30 * 8 = 240 bytes");
    println!("  Alignment/metadata: ~20 bytes");
    println!("  Total expected: ~349 bytes");
    
    // Now test with bloated Vec (growth history)
    let mut bloated = TestNList {
        n: 6,
        max_card: 50,
        no_set_list: Vec::with_capacity(100),
        remaining_cards_list: Vec::with_capacity(200),
    };
    bloated.no_set_list.extend_from_slice(&[1, 5, 10, 20, 30, 50]);
    bloated.remaining_cards_list.extend(51..81);
    
    let bytes4 = rkyv::to_bytes::<_, 256>(&bloated).unwrap();
    println!("\nTest 4 - Bloated Vec (cap=100,200): {} bytes", bytes4.len());
    println!("Bloat ratio: {:.2}x", bytes4.len() as f64 / bytes3.len() as f64);
}
