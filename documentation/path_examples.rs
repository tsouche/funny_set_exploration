// Example usage of path configuration for funny_set_exploration

use list_of_nlists::ListOfNlist;

fn main() {
    // ========================================================================
    // EXAMPLES OF PATH CONFIGURATION
    // ========================================================================
    
    // Example 1: Default - current directory
    let _list1 = ListOfNlist::new();
    // Files saved to: ./nlist_XX_batch_XXX.bin
    
    
    // Example 2: Windows - NAS drive mapped to T:
    let _list2 = ListOfNlist::with_path(r"T:\data\funny_set_exploration");
    // Files saved to: T:\data\funny_set_exploration\nlist_XX_batch_XXX.bin
    
    
    // Example 3: Windows - Local drive
    let _list3 = ListOfNlist::with_path(r"C:\Users\YourName\Documents\funny_set_data");
    // Files saved to: C:\Users\YourName\Documents\funny_set_data\nlist_XX_batch_XXX.bin
    
    
    // Example 4: Windows - UNC network path
    let _list4 = ListOfNlist::with_path(r"\\nas-server\share\funny_set");
    // Files saved to: \\nas-server\share\funny_set\nlist_XX_batch_XXX.bin
    
    
    // Example 5: Linux/macOS - Absolute path
    let _list5 = ListOfNlist::with_path("/home/username/data/funny_set");
    // Files saved to: /home/username/data/funny_set/nlist_XX_batch_XXX.bin
    
    
    // Example 6: Linux - NAS mount point
    let _list6 = ListOfNlist::with_path("/mnt/nas/data/funny_set_exploration");
    // Files saved to: /mnt/nas/data/funny_set_exploration/nlist_XX_batch_XXX.bin
    
    
    // Example 7: Cross-platform - Relative subdirectory
    let _list7 = ListOfNlist::with_path("output");
    // Files saved to: ./output/nlist_XX_batch_XXX.bin
    
    
    // Example 8: Cross-platform - Nested relative subdirectory
    let _list8 = ListOfNlist::with_path("data/nlists");
    // Files saved to: ./data/nlists/nlist_XX_batch_XXX.bin
    
    
    println!("Path configuration examples - see source code for details");
}
