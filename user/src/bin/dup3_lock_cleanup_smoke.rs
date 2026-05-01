#![no_std]
#![no_main]

#[macro_use]
extern crate user;

use user::syscall::{
    CREATE, RDWR, TRUNC, close, dup3, fork, open, pipe, read, sleep, syscall, waitpid, write,
};

const SYSCALL_FCNTL: usize = 25;
const F_SETLK: usize = 6;
const F_SETLKW: usize = 7;
const F_WRLCK: i16 = 1;
const SEEK_SET: i16 = 0;

#[repr(C)]
#[derive(Clone, Copy)]
struct Flock {
    l_type: i16,
    l_whence: i16,
    l_start: i64,
    l_len: i64,
    l_pid: i32,
}

fn write_lock(fd: usize, cmd: usize) -> isize {
    let mut lock = Flock {
        l_type: F_WRLCK,
        l_whence: SEEK_SET,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    syscall(
        SYSCALL_FCNTL,
        [fd, cmd, &mut lock as *mut Flock as usize, 0, 0, 0],
    )
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let src_fd = open("/tmp/dup3_lock_cleanup_src", RDWR | CREATE | TRUNC);
    assert!(src_fd >= 0);
    let src_fd = src_fd as usize;

    let locked_fd = open("/tmp/dup3_lock_cleanup_locked", RDWR | CREATE | TRUNC);
    assert!(locked_fd >= 0);
    let locked_fd = locked_fd as usize;
    assert_eq!(write_lock(locked_fd, F_SETLK), 0);

    let mut start_pipe = [0usize; 2];
    let mut result_pipe = [0usize; 2];
    assert_eq!(pipe(&mut start_pipe), 0);
    assert_eq!(pipe(&mut result_pipe), 0);

    let pid = fork();
    assert!(pid >= 0);
    if pid == 0 {
        close(start_pipe[1]);
        close(result_pipe[0]);
        close(src_fd);
        close(locked_fd);

        let child_fd = open("/tmp/dup3_lock_cleanup_locked", RDWR);
        assert!(child_fd >= 0);
        let child_fd = child_fd as usize;

        let mut start = [0u8; 1];
        assert_eq!(read(start_pipe[0], &mut start), 1);
        assert!(write_lock(child_fd, F_SETLK) < 0);
        assert_eq!(write(result_pipe[1], b"b"), 1);

        assert_eq!(write_lock(child_fd, F_SETLKW), 0);
        assert_eq!(write(result_pipe[1], b"a"), 1);
        close(child_fd);
        close(start_pipe[0]);
        close(result_pipe[1]);
        return 0;
    }

    close(start_pipe[0]);
    close(result_pipe[1]);
    assert_eq!(write(start_pipe[1], b"g"), 1);

    let mut result = [0u8; 1];
    assert_eq!(read(result_pipe[0], &mut result), 1);
    assert_eq!(result[0], b'b');

    sleep(50);
    assert_eq!(dup3(src_fd, locked_fd, 0), locked_fd as isize);

    assert_eq!(read(result_pipe[0], &mut result), 1);
    assert_eq!(result[0], b'a');

    close(src_fd);
    close(locked_fd);
    close(start_pipe[1]);
    close(result_pipe[0]);

    let mut exit_code = 0i32;
    assert_eq!(waitpid(pid, &mut exit_code), pid);
    assert_eq!(exit_code, 0);

    println!("dup3_lock_cleanup_smoke passed");
    0
}
