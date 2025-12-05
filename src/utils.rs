/// Few tools for debug

// This very stupid 'debug_print' function raises a problem:
//		- it is used by virtually all other modules, so it has a global scope
//		- I want to enable the other modules or main to activate/deactivate the debug printing
//			without having to re-compile the whole code, so I created the two functions 
//			'debug_print_on' and 'debug_print_off'.
//		- but for this to work, they need to share a global mutable variable 'debug_flag'...
//			which the compiler will refuse (no 'let', no 'static'...)
//
//	Grok suggested the followint solution :
//		- use AtomicBool library in order to create a global vairable while staying 'thread safe'
//			(i.e. preventing a possible race condition, in case this program is run in parallel 
//			threads (which will never happen), so that the compile will allow the code.
// This is the only way I found to enable the desired outcome !!!


use std::sync::atomic::{AtomicBool, Ordering};

// turn this constant to 'true' to print multiple debug messages
static DEBUG_FLAG: AtomicBool = AtomicBool::new(true);
static TEST_FLAG: AtomicBool = AtomicBool::new(true);

pub fn debug_print_on() {
	DEBUG_FLAG.store(true, Ordering::Relaxed);
}

pub fn test_print_on() {
	TEST_FLAG.store(true, Ordering::Relaxed);
}

pub fn debug_print_off() {
	DEBUG_FLAG.store(false, Ordering::Relaxed);
}

pub fn test_print_off() {
	TEST_FLAG.store(false, Ordering::Relaxed);
}

pub fn debug_print_noln(msg:&str) {
	if DEBUG_FLAG.load(Ordering::Relaxed) {
		eprint!("{}", format!("debug: {}", msg.to_string()));
	}
}


pub fn debug_print(msg:&str) {
	if DEBUG_FLAG.load(Ordering::Relaxed) {
		eprintln!("{}", format!("debug: {}", msg.to_string()));
	}
}

pub fn test_print(msg:&str) {
	if TEST_FLAG.load(Ordering::Relaxed) {
        eprintln!("{}", msg.to_string());
	}
}

pub fn banner(msg:&str) {
	// set the banner's width
	const BANNER_WIDTH: usize = 80; 
	// truncate the message if needed
	let msg_len = msg.len();
	let titre = if msg_len > BANNER_WIDTH {
		&msg[..BANNER_WIDTH]
	} else {
		&msg
	};
	// compute the required spaces before and after the message
	let total_padding = BANNER_WIDTH - msg_len;
	let left_padding = total_padding / 2;
	let right_padding = total_padding - left_padding;
	// Create the components of the banner
	let line = "=".repeat(BANNER_WIDTH);
	let left_spaces = " ".repeat(left_padding);
	let right_spaces = " ".repeat(right_padding);
	let banner_str = format!("\n\n{}\n{}{}{}\n{}\n\n",
		line, left_spaces, titre, right_spaces, line);
	// Display the banner
	test_print(&banner_str);
}

