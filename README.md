# funny_set_exploration
Explore cards combination to find out Set Table with up to 18 cards and no set.


## Principle of the algorithm

There are 81 cards in Set: the set cards are represented with a u8 of value 0 to 80 (included): this is enough to fully represent the cards. A set is considered valid if... (see the Set Game repositories). A table will always contain a multiple of 3 cards (_3n_ cards). Our purpose is to identify the **exhaustive** list of combination of 12, 15 and 18 cards which do NOT include any valid set. To do so we crawl all the possible combinations of 12 / 15 / 18 cards and test the presence or absence of a set.

Due to the very large number of combination of 12/15/18 cards amongst 81, it is critical to optimize the seach algorithm to be able to finish the search in a 'decent' timeframe.

- The first critical efficiency criteria is that the order of the cards does not matter when it comes to identifying a set on a table: only the values matter. So when we crawl the graph of possibilities, we do not look'backward' at the cards (i.e. since we will crawl the possibilities in increasing card value, we will not look at combinations with cards below the value of one of the card already on the table... sice such cards have already been looked at).
- The second critical efficiceny criteria is that it is cheap to compute the third card which will complement a given tuple of 2 cards to form a valid set: it is actually much cheaper than parsing all possiblilites and compute wheter all possible tripplets form a valid set.

So, considering a given list of N cards (with values from 0 to MAX):
- we create the complementary list of all 'remaining cards' (i.e. all the cards of value above MAX)
- we list all the cards which we know form a valid set with 2 cards in the list (and we store these in the list of 'forbidden' cards)
- by deduction, we can build the list of 'possible' complementary cards (all the cards between MAX+1 and 80 which are not in the 'forbidden list') which could extend the list to create a new list of N+1 cards.

Thus, increasing gradually the number of cards in the list, we reach N = 12, and then continue to N = 15 and eventually to N = 18.

## Proposed implementation:

 1. create the list seeds
 2. expand the lists from level n-1 to n until n = 18
 2. store results for n = 12, 15 and 18

### What is a list seed?

A list seed is a triplet of 3 cards which do not form a valid set. This is the minimal length of list we consider (since one need at least 3 cards to form a valid set):
- we build such a list of 3 cards, of values up to MAX:
- for any couple of card in this list, we compute the value of the 'third' card which would form a valid set with the considered couple:
    - if the value is below MAX: it was already discarded
    - if the value is above MAX: we mark this value as to be discarded in any future search
- at the end of this pass, we have a list of 3 cards, and a list of 'remaining card' which we know does not contain any card which would form a set if it were added to the list.

This combination of two lists (3 cards not forming a set, and the corresponding 'remaingin list') is a 'list seed'.

# How do we grow a list?

We start from a 'seed list' which we call a '03-list' (i.e. a list of 3 cards and the corresponding 'remaining list').

Lest' describe how - from a valid '03-list' - we will build all the possible valid '04-list', i.e. a list of 4 cards within which we cannot find any combination of 3 cards which form valid set, and the corresponding list of 'remaining cards' which do not form a valid set with any of the cards in the list of 4 cards.
More generically, let's decribe how - from a valid 'n-list' - we will build the list of all possible 'n+1-lists', with the following definition:

  A 'n-list' is a couple of lists:
      - a 'primary list' of n cards (with 3 =< n =< 18, of values =< MAX), within which we can't find any combination of 3 cards forming a valid set
      - with a list of 'remaining cards' which contains **all** the cards of value > MAX, which will not form a valid set with any couple of cards from the 'primary list'

Assuming that the 'n-list' is valid, here is how we build the list of all possible 'n+1-list':
  - for all card *C* in the remaining list:
      - create a 'n+1-primary list' with the existing 'primary list' extended with *C*
      - create a 'cadidate n+1-remaining list' for the 'primary list + *C'*:
          - start from the 'remaining card' list
          - discard any card in this remaining list of a value =< *C* : this becomes the 'candidate n+1-remaining list'
          - for any card *P* in the 'primary list':
              - compute the thid card *D* which form a valid set with *C* and *P*
              - check if *D* is in the 'candidate n+1-remaining list': if yes, remove it from the list
          - if there are not enough cards left in the 'candidate n+1-remaining list' to complement the 'primary list' to 12 cards, it means that the card C is a dead-end: drop it and go to the next card *C*
          - else you have created a valid n+1-list: store it for later processing, and move the next card *C*

Thus, from the exhaustive set of 03-lists, you create the exhaustive st of 04-lists... and so on until you reach 12 cards.

From the 12-lists, you can build teh 12-, 14- and 15-lists.

Form the 15-lists, you can build the 16-, 17- and 18-list.

We know that any able with 21 card will count multiple valid sets, so it is not usefull to ge beyond 18 cards.
We could however compute - for the fun - the list of all possible 19- and 20-lists if there are any.

