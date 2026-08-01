use crate::syscall::_yield;
// allow unused code in syscall/thread.rs:
#[allow(unused)]
use crate::syscall::syscall;
const SYSCALL_THREAD_CREATE: usize = 1000;
const SYSCALL_GETTID: usize = 1001;
const SYSCALL_WAITTID: usize = 1002;

pub fn thread_create(entry: usize, arg: usize) -> isize {
    syscall(SYSCALL_THREAD_CREATE, [entry, arg, 0, 0, 0, 0])
}
pub fn gettid() -> isize {
    syscall(SYSCALL_GETTID, [0, 0, 0, 0, 0, 0])
}
pub fn sys_waittid(tid: usize) -> isize {
    syscall(SYSCALL_WAITTID, [tid, 0, 0, 0, 0, 0])
}
pub fn waittid(tid: usize) -> isize {
    loop {
        match sys_waittid(tid) {
            -2 => {
                _yield();
            }
            exit_code => return exit_code,
        }
    }
}
