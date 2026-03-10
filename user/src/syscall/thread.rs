use crate::syscall::_yield;
// allow unused code in syscall/thread.rs:
#[allow(unused)]
use crate::syscall::syscall;
const SYSCALL_THREAD_CREATE: usize = 1000;
const SYSCALL_GETTID: usize = 1001;
const SYSCALL_WAITTID: usize = 1002;
const SYSCALL_MUTEX_CREATE: usize = 1010;
const SYSCALL_MUTEX_LOCK: usize = 1011;
const SYSCALL_MUTEX_UNLOCK: usize = 1012;
const SYSCALL_SEMAPHORE_CREATE: usize = 1020;
const SYSCALL_SEMAPHORE_UP: usize = 1021;
const SYSCALL_SEMAPHORE_DOWN: usize = 1022;
const SYSCALL_CONDVAR_CREATE: usize = 1030;
const SYSCALL_CONDVAR_SIGNAL: usize = 1031;
const SYSCALL_CONDVAR_WAIT: usize = 1032;

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
        match (sys_waittid(tid)) {
            -2 => {
                _yield();
            }
            exit_code => return exit_code,
        }
    }
}
