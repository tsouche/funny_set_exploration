/// Various helpers when manipulating Set cards
///
/// This module exposes helpers to test whether three indices form a Set,
/// compute the index that completes a set for two cards, and test whether a
/// slice of indices contains any set.

pub fn index_to_base3(i: usize) -> [usize; 4] {
    // converts a card index (0..80) to its base-3 representation
    // representing the 4 attributes of the card
    let mut rem = i;
    let mut base3 = [0; 4];
    for j in (0..4).rev() {
        base3[j] = rem % 3;
        rem = rem / 3;
    }
    return base3;
}

/// check whether the three given card form a valid Set
pub fn is_set(i0: usize, i1: usize, i2: usize) -> bool {
    let base3 = [
        index_to_base3(i0), 
        index_to_base3(i1), 
        index_to_base3(i2)
    ];
    // sum each properties (= digit of same rank) across the 3 cards
    let mut sum_base3 = [0; 4];
    for i in 0..3 {
        let b3 = base3[i];
        for j in 0..4 {
            sum_base3[j] += b3[j];
        }
    }
    // For each attribute, the sum modulo 3 must be 0 for a valid SET
    return (sum_base3[0] % 3 == 0)
        && (sum_base3[1] % 3 == 0)
        && (sum_base3[2] % 3 == 0)
        && (sum_base3[3] % 3 == 0);
}

/// Compute the card that completes the two given cards to form a valid set
pub fn next_to_set(i0: usize, i1: usize) -> usize {
    let b3_0 = index_to_base3(i0);
    let b3_1 = index_to_base3(i1);
    let mut b3_2 = [0; 4];
    for j in 0..4 {
        b3_2[j] = (3 - (b3_0[j] + b3_1[j]) % 3) % 3;
    }
    // convert back to index
    let mut index = 0;
    for j in 0..4 {
        index = index * 3 + b3_2[j];
    }
    return index;
}


