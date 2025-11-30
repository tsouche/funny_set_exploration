/// Search the grail of Set: combinations of cards with no sets
///
/// This module crawls the possible combination of 12 cards with no sets: 
/// each of these combination is stored in the vec 'no_set_12' 
/// From this list, it continues and builds all possible 15-card combinations
/// with no sets, storing them in the vec 'no_set_15'
/// From this list, it continues and builds all possible 18-card combinations
/// with no sets, storing them in the vec 'no_set_18'
/// We know that any combination of 21 cards will always have multiple sets.

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

fn contains_set(cards: &Vec<usize>) -> bool {
    // goes trough all triplets of cards and checks if they form a set
    let n = cards.len();
    for i in 0..(n - 2) {
        for j in (i + 1)..(n - 1) {
            for k in (j + 1)..n {
                if is_set(cards[i], cards[j], cards[k]) {
                    return true;
                }
            }
        }
    }
    return false;
}
 
fn look_for_no_set_03() -> Vec<Vec<usize>> {
    // create the vec to store the no-set-03 combinations
    let mut no_set_03 = Vec::new();
    // load the 81 cards
    let cards = all_cards();
    for i01 in 0..70 {
        for i02 in (i01 + 1)..71 {
            for i03 in (i02 + 1)..72 {
                let table = vec![
                    cards[i01],
                    cards[i02],
                    cards[i03],
                ];
                if !contains_set(&table) {
                    // found a no-set-03 combination
                    no_set_03.push(table);
                }
                                                }
        }
    }
    println!("Found {} no-set-03 combinations", no_set_03.len());
    return no_set_03;
}

fn build_no_set_plus_1_cards(no_set_list: &Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    let mut no_set_plus_1 = Vec::new();
    for table in no_set_list {
        // ignore all cards which are below the last card in the current table
        let max_card = table[table.len() - 1];
        // extend the table with all possible combinations of 3 remaining cards
        // and check for the existence of sets
        for card in max_card + 1..81 {
            let mut table_plus = table.clone();
            table_plus.push(card);
            if !contains_set(&table_plus) {
                // found a no-set combination with 3 more cards
                no_set_plus_1.push(table_plus);
            }
        }
    }
    println!("Found {} no-set combinations with {} cards", 
        no_set_plus_1.len(), no_set_list[0].len() + 1
    );
    return no_set_plus_1;
}

/// Build the list of all no-set-12, no-set-15 and no-set-18 combinations  
pub fn look_for_no_set_combinations() {
    // build the no-set-12 combinations list
    let no_set_03 = look_for_no_set_03();
    let no_set_04 = build_no_set_plus_1_cards(&no_set_03);
    let no_set_05 = build_no_set_plus_1_cards(&no_set_04);
    let no_set_06 = build_no_set_plus_1_cards(&no_set_05);
    let no_set_07 = build_no_set_plus_1_cards(&no_set_06);
    let no_set_08 = build_no_set_plus_1_cards(&no_set_07);
    let no_set_09 = build_no_set_plus_1_cards(&no_set_08);
    let no_set_10 = build_no_set_plus_1_cards(&no_set_09);
    let no_set_11 = build_no_set_plus_1_cards(&no_set_10);
    let no_set_12 = build_no_set_plus_1_cards(&no_set_11);
    println!("Found {} no-set-12 combinations", no_set_12.len());
    // build no-set-15 combinations
    let no_set_13 = build_no_set_plus_1_cards(&no_set_12);
    let no_set_14 = build_no_set_plus_1_cards(&no_set_13);
    let no_set_15 = build_no_set_plus_1_cards(&no_set_14);
    println!("Found {} no-set-15 combinations", no_set_15.len());
    // build no-set-18 combinations
    let no_set_18 = build_no_set_plus_1_cards(&no_set_15);
    let no_set_19 = build_no_set_plus_1_cards(&no_set_18);
    let no_set_20 = build_no_set_plus_1_cards(&no_set_19);  
    println!("Found {} no-set-18 combinations", no_set_18.len());
}