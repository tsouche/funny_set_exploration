/// Various helpers when manipulating set cards
/// 

fn all_cards() -> Vec<usize> {
    // returns a vec with all 81 card indexes (0..80)
    let mut cards = Vec::new();
    for i in 0..81 {
        cards.push(i);
    }
    return cards;
}

fn index_to_base3(i: usize) -> [usize; 4] {
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

fn is_set(i0: usize, i1: usize, i2: usize) -> bool {
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

