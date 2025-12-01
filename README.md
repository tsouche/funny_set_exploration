# funny_set_exploration
Explore cards combination to find out Set Table with 18+ cards and no set

Current approach:
- brute force exploration of the tree of possible combination, starting from triplets of cards (so that sets are eliminated early in the process).
- order does not matter, so we crawl only with ascending values, not looking back at values which are below the 'max value being in the list)
- batch the processeing in lots of 10 million tries...
 
To be changed to:
- it is cheaper to calculate which card will be a set for a given couple of cards than to try every other cards and keep only the ones which are not a set
- so start a list of 81 cards:
- for each couple of card (starting from the beginning): compute the thrid one, and bar it from the list of 'remaingin cards'
  - thus, each time you consider a couple of cards at the beginning of the list, bar more cards in the list of remaining cars to be examined

 To be refined...
