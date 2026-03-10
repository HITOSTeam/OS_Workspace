#![no_std]
#![no_main]

use alloc::vec::Vec;
use alloc::{string::String, vec};
use user::{
    println,
    syscall::{exec, fork, waitpid},
};
extern crate alloc;
fn run_tests(test: Vec<&str>) {
    for test in test {
        let child = fork();
        if child == 0 {
            // parse args:
            if test.contains(' ') {
                let mut args: Vec<&str> = test.split(' ').collect();
                let app_name = args.remove(0);

                // Build full path with / prefix and null terminator
                let mut full_path = String::from("");
                full_path.push_str(app_name);
                full_path.push('\0');

                // Store the String objects to keep them alive
                let mut arg_strings: Vec<String> = vec![];
                let mut cstr_args: Vec<*const u8> = vec![];

                for arg in args.iter() {
                    let mut arg_cstr = String::from(*arg);
                    arg_cstr.push('\0');
                    cstr_args.push(arg_cstr.as_ptr());
                    arg_strings.push(arg_cstr); // Keep the String alive
                }
                cstr_args.push(0 as *const u8); // null-terminate
                exec(&full_path, &cstr_args);
            } else {
                // For simple test names, add / prefix
                let mut full_path = String::from("");
                full_path.push_str(test.trim_end_matches('\0'));
                full_path.push('\0');
                exec(&full_path, &[]);
            }
        } else {
            let mut _exitcode = 0;
            let _result = waitpid(-1, &mut _exitcode);
            assert_eq!(_exitcode, 0);
        }
    }
}
#[unsafe(no_mangle)]
unsafe fn main() -> usize {
    let app_names = vec![
        vec!["pipe_test\0"],
        vec!["thread_arg\0", "thread\0", "thread_counter\0"],
        vec!["mutex_block 1 1\0", "mutex_block 4 4\0"],
        vec!["producer\0"],
        vec!["semaphore_basic\0", "semaphore_cond\0"],
        vec!["sleep_simple\0", "sleep_stress\0", "sleep_test\0"],
    ];

    for group in app_names {
        run_tests(group.to_vec());
    }
    println!("===============================");
    println!("[ALL_TESTS] All tests passed!");
    return 0;
}
