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

use crate::is_set::*;
use std::cmp::min;

#[derive(Clone)]
pub struct NList {
    pub n: u8,
    pub max_card: usize,
    pub no_set_list: Vec<usize>,
    pub remaining_cards_list: Vec<usize>,
}


impl NList {
    /// return a string representation of the no-set-list
    pub fn to_string(&self) -> String {
        // check there are at least 3 cards in no-set-list
        let nsl_len = self.no_set_list.len();
        if nsl_len < 3 {
            return "invalid".to_string();
        }
        // build no-set-list message
        let mut nsl_msg = "(".to_string();
        for i in &self.no_set_list {
            nsl_msg.push_str(&format!("{:>2}", i));
            if i + 1 < nsl_len {
                nsl_msg.push_str(",");
            }
        }
        nsl_msg.push_str(")");
        // build remaining cards list message
        let rcl_len = self.remaining_cards_list.len();
        let mut rcl_msg = "[".to_string();
        if rcl_len == 0 {
            rcl_msg.push_str("...");
        } else {
            for i in 0..rcl_len  {
                rcl_msg.push_str(&format!("{:>2}", self.remaining_cards_list[i]));
                if i + 1 < rcl_len {
                    rcl_msg.push_str(",");
                }
            }
        }
        rcl_msg.push_str("]");
        // consolidate the whole string
        return format!("{:>2}-list: max={:>2} : {}+{}", self.n, self.max_card, nsl_msg, rcl_msg);
    }

    /// Return a list of n+1-no_set_lists built from the current n-no_set_list
    /// Implementation note:
    ///     - for all card C in the remaining list:
    ///         - create a 'n+1-primary list' with the existing 'primary list' 
    ///           extended with C
    ///      - create a 'cadidate n+1-remaining list' for the 'primary list + C':
    ///          - start from the 'remaining card' list
    ///          - discard any card in this remaining list of a value =< C : this becomes the 'candidate n+1-remaining list'
    ///          - for any card P in the 'primary list':
    ///              - compute the thid card D which form a valid set with C and P
    ///              - check if D is in the 'candidate n+1-remaining list': if yes, remove it from the list
    ///          - if there are not enough cards left in the 'candidate n+1-remaining list' to complement the 'primary list' to 12 cards, it means that the card C is a dead-end: drop it and go to the next card C
    ///          - else you have created a valid n+1-list: store it for later processing, and move the next card C
    ///   - return the list of n+1-no_set_lists created
    pub fn build_n_plus_1_no_set_lists(&self) -> Vec<NList> {
        // we will store the resulting n+1-no_set_lists here
        let mut n_plus_1_lists = Vec::new();
        // for all card C in the remaining list
        for c in self.remaining_cards_list.iter() {
            // create the n+1-primary list
            let mut n_plus_1_primary_list = self.no_set_list.clone();
            n_plus_1_primary_list.push(*c);
            // create the candidate n+1-remaining list (all cards above c)
            let mut n_plus_1_remaining_list: Vec<usize> = self
                .remaining_cards_list
                .iter()
                .filter(|&&x| x > *c)
                .cloned()
                .collect();
            // for all card P in the primary list, remove from the candidate 
            // remaining list any D card that would form a valid set with C and
            // P
            for p in self.no_set_list.iter() {
                let d = next_to_set(*p, *c);
                n_plus_1_remaining_list.retain(|&x| x != d);
            }
            // check if we have enough cards left in the candidate remaining list
            let cards_needed = 12 - min(self.n as usize + 1, 12);
            if n_plus_1_remaining_list.len() >= cards_needed {
                // we have created a valid n+1-no_set_list: store it
                let n_plus_1_nlist = NList {
                    n: self.n + 1,
                    max_card: *c,
                    no_set_list: n_plus_1_primary_list,
                    remaining_cards_list: n_plus_1_remaining_list,
                };
                n_plus_1_lists.push(n_plus_1_nlist);
            }
        }
        return n_plus_1_lists;
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
    // create the no-set-03 combinations (i < 70 to get to at least 12 cards)
    for i in 0..70 {
        for j in (i + 1)..71 {
            for k in (j + 1)..72 {
                // (i,j,k) is a candidate for a no-set-03 combination
                let table = vec![i, j, k];
                if !is_set(i, j, k) {
                    // (i,j,k) is a no-set-03 combination
                    // build a 'remaining list' with all the possible values strictly greater than k
                    let mut remaining_cards: Vec<usize> = (k + 1..81).collect();
                    // remove from this list all cards that would create a set
                    // with any pair of cards in the current table
                    let c1 = next_to_set(i, j);
                    let c2 = next_to_set(i, k);
                    let c3 = next_to_set(j, k);
                    remaining_cards.retain(|&x| x != c1 && x != c2 && x != c3);
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
