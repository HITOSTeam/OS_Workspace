use core::arch::asm;

use alloc::string::String;

const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_GET_TIME: usize = 169;
const SYSCALL_GETPID: usize = 172;
const SYSCALL_FORK: usize = 220;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_WAITPID: usize = 260;
const SYSCALL_GETCWD: usize = 17;
const SYSCALL_CHDIR: usize = 49;
const SYSCALL_OPENAT: usize = 56;
const SYSCALL_CLOSE: usize = 57;
const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_UMOUNT2: usize = 39;
const SYSCALL_SYNC: usize = 81;
const SYSCALL_PIPE2: usize = 59;
const SYSCALL_DUP3: usize = 24;
const SYSCALL_EVENTFD2: usize = 19;
const SYSCALL_EPOLL_CREATE1: usize = 20;
const SYSCALL_EPOLL_CTL: usize = 21;
const SYSCALL_EPOLL_PWAIT: usize = 22;
const SYSCALL_MQ_OPEN: usize = 180;
const SYSCALL_MQ_UNLINK: usize = 181;
const SYSCALL_MQ_TIMEDSEND: usize = 182;
const SYSCALL_MQ_TIMEDRECEIVE: usize = 183;
const SYSCALL_MQ_NOTIFY: usize = 184;
const SYSCALL_MQ_GETSETATTR: usize = 185;
const SYSCALL_TIMERFD_CREATE: usize = 85;
const SYSCALL_TIMERFD_SETTIME: usize = 86;
const SYSCALL_TIMERFD_GETTIME: usize = 87;
const SYSCALL_GETDENTS64: usize = 61;
const SYSCALL_REBOOT: usize = 142;

const SYSCALL_SIGACTION: usize = 134;
const SYSCALL_SIGPROCMASK: usize = 135;
const SYSCALL_SIGRETURN: usize = 139;
const SYSCALL_KILL: usize = 129;

pub const SYSCALL_FORTEST: usize = 1000;
mod mutex;
mod thread;
pub use mutex::*;
pub use thread::*;
const SYSCALL_GET_HARTID: usize = 998;
#[cfg(target_arch = "riscv64")]
pub fn syscall(id: usize, args: [usize; 6]) -> isize {
    let mut ret: isize;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x13") args[3],
            in("x14") args[4],
            in("x15") args[5],
            in("x17") id
        );
    }
    ret
}

#[cfg(target_arch = "loongarch64")]
pub fn syscall(id: usize, args: [usize; 6]) -> isize {
    let mut ret: isize;
    unsafe {
        asm!(
            "syscall 0",
            inlateout("$r4") args[0] => ret,
            in("$r5") args[1],
            in("$r6") args[2],
            in("$r7") args[3],
            in("$r8") args[4],
            in("$r9") args[5],
            in("$r11") id
        );
    }
    ret
}

#[cfg(not(any(target_arch = "riscv64", target_arch = "loongarch64")))]
compile_error!("unsupported target_arch for user syscall");

fn sys_read(fd: usize, buf: usize, len: usize) -> isize {
    syscall(SYSCALL_READ, [fd, buf, len, 0, 0, 0])
}
pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf.as_mut_ptr() as usize, buf.len())
}
pub fn getchar() -> u8 {
    let mut buf = [0u8; 1];
    sys_read(0, buf.as_mut_ptr() as usize, 1);
    buf[0]
}
fn sys_write(fd: usize, buf: usize, len: usize) -> isize {
    syscall(SYSCALL_WRITE, [fd, buf, len, 0, 0, 0])
}
fn sys_exit(code: usize) -> ! {
    syscall(SYSCALL_EXIT, [code as usize, 0, 0, 0, 0, 0]);
    panic!("nerver return! exit HERE!")
}
pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf.as_ptr() as usize, buf.len())
}
pub fn syscall_fortest(a: usize, b: usize) -> isize {
    syscall(SYSCALL_FORTEST, [a, b, 0, 0, 0, 0])
}
pub fn exit(code: isize) -> ! {
    sys_exit(code as usize)
}
pub fn _yield() {
    syscall(SYSCALL_YIELD, [0, 0, 0, 0, 0, 0]);
}
fn sys_get_time() -> isize {
    syscall(SYSCALL_GET_TIME, [0, 0, 0, 0, 0, 0])
}
pub fn get_time() -> isize {
    sys_get_time()
}
pub fn sleep(period_ms: usize) {
    #[repr(C)]
    struct TimeVal {
        sec: u64,
        usec: u64,
    }
    const SYSCALL_NANOSLEEP: usize = 101;
    let tv = TimeVal {
        sec: (period_ms / 1000) as u64,
        usec: ((period_ms % 1000) * 1000) as u64,
    };
    let _ = syscall(
        SYSCALL_NANOSLEEP,
        [&tv as *const TimeVal as usize, 0, 0, 0, 0, 0],
    );
}

fn sys_fork() -> isize {
    // Linux-style fork is implemented via clone(SIGCHLD, ...).
    syscall(220, [17, 0, 0, 0, 0, 0])
}
pub fn fork() -> isize {
    sys_fork()
}
pub fn waitpid(pid_or_ne: isize, exit_code: &mut i32) -> isize {
    syscall(
        SYSCALL_WAITPID,
        [
            pid_or_ne as usize,
            exit_code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        ],
    )
}
pub fn wait(exit_code: &mut i32) -> isize {
    waitpid(-1, exit_code)
}
// attentio: if no args pass [null] instead of empty []
pub fn exec(path: &str, args_addr: &[*const u8]) -> isize {
    // dont need to pass the length of path, as it is ended with '\0'
    let mut args_addr = args_addr;
    if args_addr.is_empty() {
        args_addr = &[core::ptr::null()];
    }
    syscall(
        SYSCALL_EXEC,
        [
            path.as_ptr() as usize,
            args_addr.as_ptr() as usize,
            0,
            0,
            0,
            0,
        ],
    )
}

pub fn execve(path: &str, args_addr: &[*const u8], env_addr: &[*const u8]) -> isize {
    let mut args_addr = args_addr;
    if args_addr.is_empty() {
        args_addr = &[core::ptr::null()];
    }
    let mut env_addr = env_addr;
    if env_addr.is_empty() {
        env_addr = &[core::ptr::null()];
    }
    syscall(
        SYSCALL_EXEC,
        [
            path.as_ptr() as usize,
            args_addr.as_ptr() as usize,
            env_addr.as_ptr() as usize,
            0,
            0,
            0,
        ],
    )
}
pub fn getpid() -> isize {
    syscall(SYSCALL_GETPID, [0, 0, 0, 0, 0, 0])
}

pub fn sync() -> isize {
    syscall(SYSCALL_SYNC, [0, 0, 0, 0, 0, 0])
}

pub fn mkdirat(dirfd: isize, path: &str, mode: usize) -> isize {
    syscall(
        SYSCALL_MKDIRAT,
        [dirfd as usize, path.as_ptr() as usize, mode, 0, 0, 0],
    )
}

pub fn mount(source: &str, target: &str, fs_type: &str, flags: usize) -> isize {
    syscall(
        SYSCALL_MOUNT,
        [
            source.as_ptr() as usize,
            target.as_ptr() as usize,
            fs_type.as_ptr() as usize,
            flags,
            0,
            0,
        ],
    )
}

pub fn umount2(target: &str, flags: usize) -> isize {
    syscall(
        SYSCALL_UMOUNT2,
        [target.as_ptr() as usize, flags, 0, 0, 0, 0],
    )
}

pub fn poweroff() -> ! {
    let _ = syscall(SYSCALL_REBOOT, [0, 0, 0, 0, 0, 0]);
    loop {}
}
pub fn get_hartid() -> isize {
    syscall(SYSCALL_GET_HARTID, [0, 0, 0, 0, 0, 0])
}
pub const RDONLY: usize = 0;
///Write only
pub const WRONLY: usize = 1 << 0;
///Read & Write
pub const RDWR: usize = 1 << 1;
///Allow create
pub const CREATE: usize = 1 << 9;
///Clear file and return an empty one
pub const TRUNC: usize = 1 << 10;

const AT_FDCWD: isize = -100;
pub const CLOCK_REALTIME: usize = 0;
pub const CLOCK_MONOTONIC: usize = 1;
pub const TFD_NONBLOCK: usize = 0x800;
pub const TFD_CLOEXEC: usize = 0x80000;
pub const TFD_TIMER_ABSTIME: usize = 0x1;
pub const TFD_TIMER_CANCEL_ON_SET: usize = 0x2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TimeSpec {
    pub sec: i64,
    pub nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ITimerSpec {
    pub it_interval: TimeSpec,
    pub it_value: TimeSpec,
}
const O_CREAT: usize = 0x40;
const O_EXCL: usize = 0x80;
const O_NONBLOCK: usize = 0x800;
const O_TRUNC: usize = 0x200;
pub const MQ_O_CREAT: usize = O_CREAT;
pub const MQ_O_EXCL: usize = O_EXCL;
pub const MQ_O_NONBLOCK: usize = O_NONBLOCK;
pub const SIGEV_SIGNAL: i32 = 0;
pub const SIGEV_NONE: i32 = 1;
pub const SIGEV_THREAD: i32 = 2;
pub const SIGEV_THREAD_ID: i32 = 4;

pub const EPOLL_CTL_ADD: usize = 1;
pub const EPOLL_CTL_DEL: usize = 2;
pub const EPOLL_CTL_MOD: usize = 3;
pub const EFD_SEMAPHORE: usize = 0x1;
pub const EFD_NONBLOCK: usize = 0x800;
pub const EFD_CLOEXEC: usize = 0x80000;
pub const EPOLLIN: u32 = 0x001;
pub const EPOLLOUT: u32 = 0x004;
pub const EPOLLERR: u32 = 0x008;
pub const EPOLLHUP: u32 = 0x010;
pub const EPOLLRDHUP: u32 = 0x2000;
pub const EPOLLONESHOT: u32 = 1u32 << 30;
pub const EPOLLET: u32 = 1u32 << 31;
pub const EPOLL_CLOEXEC: usize = 0x80000;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct EpollEvent {
    pub events: u32,
    pub data: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MqAttr {
    pub mq_flags: i64,
    pub mq_maxmsg: i64,
    pub mq_msgsize: i64,
    pub mq_curmsgs: i64,
    pub __reserved: [i64; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sigevent {
    pub sigev_value: usize,
    pub sigev_signo: i32,
    pub sigev_notify: i32,
    pub sigev_data: [usize; 2],
}

fn to_linux_open_flags(flags: usize) -> usize {
    let mut out = flags & 0x3; // access mode compatible with Linux
    if (flags & CREATE) != 0 {
        out |= O_CREAT;
    }
    if (flags & TRUNC) != 0 {
        out |= O_TRUNC;
    }
    out
}

pub fn open(file_path: &str, open_flags: usize) -> isize {
    openat(AT_FDCWD, file_path, open_flags, 0)
}

pub fn openat(dirfd: isize, file_path: &str, open_flags: usize, mode: usize) -> isize {
    extern crate alloc;
    use alloc::ffi::CString;
    let Ok(cstr) = CString::new(file_path) else {
        return -1;
    };
    syscall(
        SYSCALL_OPENAT,
        [
            dirfd as usize,
            cstr.as_ptr() as usize,
            to_linux_open_flags(open_flags),
            mode,
            0,
            0,
        ],
    )
}
pub fn close(fd: usize) -> isize {
    syscall(SYSCALL_CLOSE, [fd, 0, 0, 0, 0, 0])
}

pub fn pipe(pipe: &mut [usize; 2]) -> isize {
    // Keep Linux-compatible layout (int[2]) for C tests while exposing usize fds to Rust apps.
    let mut tmp = [0i32; 2];
    let ret = syscall(SYSCALL_PIPE2, [tmp.as_mut_ptr() as usize, 0, 0, 0, 0, 0]);
    if ret == 0 {
        pipe[0] = tmp[0] as usize;
        pipe[1] = tmp[1] as usize;
    }
    ret
}

pub fn eventfd(initval: u64, flags: usize) -> isize {
    syscall(SYSCALL_EVENTFD2, [initval as usize, flags, 0, 0, 0, 0])
}

pub fn mq_open(name: &str, oflag: usize, mode: usize, attr: Option<&MqAttr>) -> isize {
    extern crate alloc;
    use alloc::ffi::CString;
    let Ok(cstr) = CString::new(name) else {
        return -1;
    };
    let attr_ptr = attr.map_or(0usize, |v| v as *const MqAttr as usize);
    syscall(
        SYSCALL_MQ_OPEN,
        [cstr.as_ptr() as usize, oflag, mode, attr_ptr, 0, 0],
    )
}

pub fn mq_unlink(name: &str) -> isize {
    extern crate alloc;
    use alloc::ffi::CString;
    let Ok(cstr) = CString::new(name) else {
        return -1;
    };
    syscall(SYSCALL_MQ_UNLINK, [cstr.as_ptr() as usize, 0, 0, 0, 0, 0])
}

pub fn mq_timedsend(mqdes: usize, msg: &[u8], prio: u32, abs_timeout: Option<&TimeSpec>) -> isize {
    let timeout_ptr = abs_timeout.map_or(0usize, |v| v as *const TimeSpec as usize);
    syscall(
        SYSCALL_MQ_TIMEDSEND,
        [
            mqdes,
            msg.as_ptr() as usize,
            msg.len(),
            prio as usize,
            timeout_ptr,
            0,
        ],
    )
}

pub fn mq_timedreceive(
    mqdes: usize,
    buf: &mut [u8],
    prio: Option<&mut u32>,
    abs_timeout: Option<&TimeSpec>,
) -> isize {
    let prio_ptr = prio.map_or(0usize, |v| v as *mut u32 as usize);
    let timeout_ptr = abs_timeout.map_or(0usize, |v| v as *const TimeSpec as usize);
    syscall(
        SYSCALL_MQ_TIMEDRECEIVE,
        [
            mqdes,
            buf.as_mut_ptr() as usize,
            buf.len(),
            prio_ptr,
            timeout_ptr,
            0,
        ],
    )
}

pub fn mq_notify(mqdes: usize, notification: Option<&Sigevent>) -> isize {
    let notification_ptr = notification.map_or(0usize, |ev| ev as *const Sigevent as usize);
    syscall(SYSCALL_MQ_NOTIFY, [mqdes, notification_ptr, 0, 0, 0, 0])
}

pub fn mq_getattr(mqdes: usize, attr: &mut MqAttr) -> isize {
    syscall(
        SYSCALL_MQ_GETSETATTR,
        [mqdes, 0, attr as *mut MqAttr as usize, 0, 0, 0],
    )
}

pub fn timerfd_create(clockid: usize, flags: usize) -> isize {
    syscall(SYSCALL_TIMERFD_CREATE, [clockid, flags, 0, 0, 0, 0])
}

pub fn timerfd_settime(
    fd: usize,
    flags: usize,
    new_value: &ITimerSpec,
    old_value: Option<&mut ITimerSpec>,
) -> isize {
    let old_value_ptr = old_value.map_or(0usize, |spec| spec as *mut ITimerSpec as usize);
    syscall(
        SYSCALL_TIMERFD_SETTIME,
        [
            fd,
            flags,
            new_value as *const ITimerSpec as usize,
            old_value_ptr,
            0,
            0,
        ],
    )
}

pub fn timerfd_gettime(fd: usize, curr_value: &mut ITimerSpec) -> isize {
    syscall(
        SYSCALL_TIMERFD_GETTIME,
        [fd, curr_value as *mut ITimerSpec as usize, 0, 0, 0, 0],
    )
}

pub fn epoll_create1(flags: usize) -> isize {
    syscall(SYSCALL_EPOLL_CREATE1, [flags, 0, 0, 0, 0, 0])
}

pub fn epoll_ctl(epfd: usize, op: usize, fd: usize, event: Option<&EpollEvent>) -> isize {
    let event_ptr = event.map_or(0usize, |ev| ev as *const EpollEvent as usize);
    syscall(SYSCALL_EPOLL_CTL, [epfd, op, fd, event_ptr, 0, 0])
}

pub fn epoll_wait(epfd: usize, events: &mut [EpollEvent], timeout_ms: isize) -> isize {
    syscall(
        SYSCALL_EPOLL_PWAIT,
        [
            epfd,
            events.as_mut_ptr() as usize,
            events.len(),
            timeout_ms as usize,
            0,
            0,
        ],
    )
}

pub fn dup3(oldfd: usize, newfd: usize, flags: usize) -> isize {
    syscall(SYSCALL_DUP3, [oldfd, newfd, flags, 0, 0, 0])
}

pub fn chdir(path: &str) -> isize {
    extern crate alloc;
    use alloc::ffi::CString;
    let Ok(cstr) = CString::new(path) else {
        return -1;
    };
    syscall(SYSCALL_CHDIR, [cstr.as_ptr() as usize, 0, 0, 0, 0, 0])
}

pub fn getcwd() -> String {
    let mut buf = alloc::vec![0u8; 256];
    let ret = syscall(
        SYSCALL_GETCWD,
        [buf.as_mut_ptr() as usize, buf.len(), 0, 0, 0, 0],
    );
    if ret < 0 {
        return String::from("?");
    }
    let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..nul]).into_owned()
}

pub fn getdents64(fd: usize, buf: &mut [u8]) -> isize {
    syscall(
        SYSCALL_GETDENTS64,
        [fd, buf.as_mut_ptr() as usize, buf.len(), 0, 0, 0],
    )
}

pub fn sigreturn() -> isize {
    syscall(SYSCALL_SIGRETURN, [0, 0, 0, 0, 0, 0])
}
pub fn kill(pid: usize, signum: i32) -> isize {
    syscall(SYSCALL_KILL, [pid, signum as usize, 0, 0, 0, 0])
}
use bitflags::bitflags;

bitflags! {
    pub struct SignalFlags: u32 {
        const SIGDEF = 1; // Default signal handling
        const SIGHUP = 1 << 1;
        const SIGINT = 1 << 2;
        const SIGQUIT = 1 << 3;
        const SIGILL = 1 << 4;
        const SIGTRAP = 1 << 5;
        const SIGABRT = 1 << 6;
        const SIGBUS = 1 << 7;
        const SIGFPE = 1 << 8;
        const SIGKILL = 1 << 9;
        const SIGUSR1 = 1 << 10;
        const SIGSEGV = 1 << 11;
        const SIGUSR2 = 1 << 12;
        const SIGPIPE = 1 << 13;
        const SIGALRM = 1 << 14;
        const SIGTERM = 1 << 15;
        const SIGSTKFLT = 1 << 16;
        const SIGCHLD = 1 << 17;
        const SIGCONT = 1 << 18;
        const SIGSTOP = 1 << 19;
        const SIGTSTP = 1 << 20;
        const SIGTTIN = 1 << 21;
        const SIGTTOU = 1 << 22;
        const SIGURG = 1 << 23;
        const SIGXCPU = 1 << 24;
        const SIGXFSZ = 1 << 25;
        const SIGVTALRM = 1 << 26;
        const SIGPROF = 1 << 27;
        const SIGWINCH = 1 << 28;
        const SIGIO = 1 << 29;
        const SIGPWR = 1 << 30;
        const SIGSYS = 1 << 31;
    }
}
pub const SIGDEF: i32 = 0; // Default signal handling
pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
pub const SIGBUS: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGUSR1: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGUSR2: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGSTKFLT: i32 = 16;
pub const SIGCHLD: i32 = 17;
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;
pub const SIGTSTP: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;
pub const SIGURG: i32 = 23;
pub const SIGXCPU: i32 = 24;
pub const SIGXFSZ: i32 = 25;
pub const SIGVTALRM: i32 = 26;
pub const SIGPROF: i32 = 27;
pub const SIGWINCH: i32 = 28;
pub const SIGIO: i32 = 29;
pub const SIGPWR: i32 = 30;
pub const SIGSYS: i32 = 31;
pub struct SignalAction {
    pub handler: usize,
    pub mask: SignalFlags,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RtSigAction {
    handler: usize,
    flags: usize,
    restorer: usize,
    mask: u64,
}

impl Default for SignalAction {
    fn default() -> Self {
        SignalAction {
            handler: 0,
            mask: SignalFlags::empty(),
        }
    }
}
pub fn sigaction(
    signum: i32,
    action: Option<&SignalAction>,
    old_action: Option<&mut SignalAction>,
) -> isize {
    let action_rt = action.map(|act| RtSigAction {
        handler: act.handler,
        flags: 0,
        restorer: 0,
        mask: act.mask.bits() as u64,
    });
    let mut old_rt = RtSigAction::default();
    let action_ptr = action_rt
        .as_ref()
        .map_or(core::ptr::null(), |act| act as *const RtSigAction);
    let old_action_ptr = if old_action.is_some() {
        &mut old_rt as *mut RtSigAction
    } else {
        core::ptr::null_mut()
    };
    let ret = syscall(
        SYSCALL_SIGACTION,
        [
            signum as usize,
            action_ptr as usize,
            old_action_ptr as usize,
            core::mem::size_of::<u64>(),
            0,
            0,
        ],
    );
    if ret >= 0 {
        if let Some(old_act) = old_action {
            old_act.handler = old_rt.handler;
            old_act.mask = SignalFlags::from_bits_truncate(old_rt.mask as u32);
        }
    }
    ret
}
pub fn sigprocmask(how: u32) -> isize {
    syscall(SYSCALL_SIGPROCMASK, [how as usize, 0, 0, 0, 0, 0])
}
