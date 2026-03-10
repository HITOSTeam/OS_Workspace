use crate::syscall::syscall;

const SYSCALL_MUTEX_CREATE: usize = 1010;
const SYSCALL_MUTEX_LOCK: usize = 1011;
const SYSCALL_MUTEX_UNLOCK: usize = 1012;
const SYSCALL_SEMAPHORE_CREATE: usize = 1020;
const SYSCALL_SEMAPHORE_UP: usize = 1021;
const SYSCALL_SEMAPHORE_DOWN: usize = 1022;
const SYSCALL_CONDVAR_CREATE: usize = 1030;
const SYSCALL_CONDVAR_SIGNAL: usize = 1031;
const SYSCALL_CONDVAR_WAIT: usize = 1032;
pub fn sys_mutex_create(is_blocking: bool) -> isize {
    syscall(SYSCALL_MUTEX_CREATE, [is_blocking as usize, 0, 0, 0, 0, 0])
}
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    syscall(SYSCALL_MUTEX_LOCK, [mutex_id, 0, 0, 0, 0, 0])
}
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    syscall(SYSCALL_MUTEX_UNLOCK, [mutex_id, 0, 0, 0, 0, 0])
}
pub fn sys_semaphore_create(res_count: usize) -> isize {
    syscall(SYSCALL_SEMAPHORE_CREATE, [res_count, 0, 0, 0, 0, 0])
}
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    syscall(SYSCALL_SEMAPHORE_UP, [sem_id, 0, 0, 0, 0, 0])
}
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    syscall(SYSCALL_SEMAPHORE_DOWN, [sem_id, 0, 0, 0, 0, 0])
}
pub fn sys_condvar_create() -> isize {
    syscall(SYSCALL_CONDVAR_CREATE, [0, 0, 0, 0, 0, 0])
}
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    syscall(SYSCALL_CONDVAR_SIGNAL, [condvar_id, 0, 0, 0, 0, 0])
}
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    syscall(SYSCALL_CONDVAR_WAIT, [condvar_id, mutex_id, 0, 0, 0, 0])
}
pub fn mutex_create() -> isize {
    sys_mutex_create(false)
}
pub fn mutex_blocking_create() -> isize {
    sys_mutex_create(true)
}
pub fn mutex_lock(mutex_id: usize) {
    sys_mutex_lock(mutex_id);
}
pub fn mutex_unlock(mutex_id: usize) {
    sys_mutex_unlock(mutex_id);
}
pub fn semaphore_create(res_count: usize) -> isize {
    sys_semaphore_create(res_count)
}
pub fn semaphore_up(sem_id: usize) {
    sys_semaphore_up(sem_id);
}
pub fn semaphore_down(sem_id: usize) {
    sys_semaphore_down(sem_id);
}
pub fn condvar_create() -> isize {
    sys_condvar_create()
}
pub fn condvar_signal(condvar_id: usize) {
    sys_condvar_signal(condvar_id);
}
pub fn condvar_wait(condvar_id: usize, mutex_id: usize) {
    sys_condvar_wait(condvar_id, mutex_id);
}
