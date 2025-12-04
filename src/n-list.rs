/// This module enable to manage a 'n-list', i.e. a list of n-sized combinations
/// of set cards (of value from 0 to 80):
///     - within which no valid set can be found
///     - with the corresponding list of 'remaining cards' that can be added to 
///       the n-sized combinations without creating a valid set
/// 
/// The methods provided here are used to build such n-lists incrementally,
/// starting from no-set-03 combinations, then no-set-04, no-set-05, etc...
/// 
/// The main function is `build_n+1_set()` which builds the list of all possible
/// no-set-n+1 from a given no-set-n list.

use crate::is_set::contains_set;

pub struct NList {
    pub n: u8,
    pub max_card: usize,
    pub no_set_list: Vec<usize>,
    pub remaining_cards_list: Vec<usize>,
}

impl NList {
    pub fn to_string(&self) {
        let mut nsl_msg = "(".to_string();
        for i in no_set_list {
            nsl_msg.push_str(&format!("{:>2}", i));
            if i < no_set_list.len() - 1 {
                nsl_msg.push_str(",");
            }
        }
        nsl_msg.push_str(")");
        let mut rcl_msg = " [".to_string();
        for i in remaining_cards_list {
            rcl_msg.push_str(&format!("{:>2}", i));
            if i < remaining_cards_list.len() - 1 {
                rcl_msg.push_str(",");
            }
        }
        rcl_msg.push_str("]");
        return format!("{:>2}-list: max={:>2} : {}+{}", self.n, self.max_card, nsl_msg, rcl_msg);
    }
}


/// Build the list of all possible no-set-03 combinations, i.e. combinations of 
/// 3 cards within which no valid set can be found, with their corresponding 
/// remaining cards list.
/// 
/// NB:
///     - knowing that we will need to have at least 12 cards on the table 
///       eventually, we limit the max card index to 72 (i.e. one will need to 
///       complement the 3 cards with at least 9 more coards to get to 12).
///     - if we want to focus on the no-set-table with 15 cards, we may stop at
///       max card index 68 (i.e. one will need to complement the 3 cards with
///       at least 12 more cards to get to 15).
///     - if we want to focus on the no-set-table with 18 cards, we may stop at
///       max card index 65 (i.e. one will need to complement the 3 cards with
///       at least 15 more cards to get to 18).

pub fn create_all_03_no_set_lists() -> Vec<NList> {
    // we will store the results in this vector
    let mut no_set_03 = Vec::new();
    // create the no-set-03 combinations: knowing that we will need to have at least 12 cards on the table eventually, we limit the max card index to 72 (i.e. one will need to complement the 3 cards with at least 9 more coards to get to 12).
    for i in 0..70 {
        for j in (i + 1)..71 {
            for k in (j + 1)..72 {
                // (i,j,k) is a candidate for a no-set-03 combination
                let table = vec![i, j, k];
                if !contains_set(&table) {
                    // found a no-set-03 combination
                    // build a 'remainign list' with all the possible values 
                    let mut remaining_cards = vec![k+1..81];
                    // remove from this list all cards that would create a set
                    // with any pair of cards in the current table
                    let c1 = next_to_set(i, j);
                    let c2 = next_to_set(i, k);
                    let c3 = next_to_set(j, k);
                    if remaining_cards.contains(&c1) {
                        remaining_cards.retain(|&x| x != c1);
                    }
                    if remaining_cards.contains(&c2) {
                        remaining_cards.retain(|&x| x != c2);
                    }
                    if remaining_cards.contains(&c3) {
                        remaining_cards.retain(|&x| x != c3);
                    }
                    // store the resulting n-list
                    let nlist = NList {
                        n: 3,
                        max_card: k,
                        no_set_list: table,
                        remaining_cards_list: remaining_cards,
                    };
                    no_set_03.push(nlist);
                }
            }
        }
    }
    return no_set_03;
}
