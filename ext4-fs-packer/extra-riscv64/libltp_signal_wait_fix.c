typedef unsigned long uintptr_t;
typedef unsigned long sigset_t;
typedef long time_t;
typedef void (*sighandler_t)(int);

#define SYS_rt_sigaction 134
#define SYS_rt_sigprocmask 135
#define SYS_rt_sigtimedwait 137
#define SYS_rt_sigreturn 139

#define EINVAL 22
#define SIGKILL 9
#define SIGSTOP 19
#define SIG_ERR ((sighandler_t)-1)

#define SIG_BLOCK 0
#define SIG_UNBLOCK 1
#define SA_RESTORER 0x04000000UL
#define SA_RESTART 0x10000000UL

struct timespec {
    time_t tv_sec;
    long tv_nsec;
};

struct kernel_sigaction {
    sighandler_t handler;
    unsigned long flags;
    void (*restorer)(void);
    sigset_t mask;
};

extern int *__errno_location(void);

void __ltp_rt_restore(void);
__asm__(
    ".global __ltp_rt_restore\n"
    "__ltp_rt_restore:\n"
    "li a7, 139\n"
    "ecall\n"
);

static inline long raw_syscall4(long n, long a0, long a1, long a2, long a3) {
    register long x10 __asm__("a0") = a0;
    register long x11 __asm__("a1") = a1;
    register long x12 __asm__("a2") = a2;
    register long x13 __asm__("a3") = a3;
    register long x17 __asm__("a7") = n;
    __asm__ volatile("ecall"
                     : "+r"(x10)
                     : "r"(x11), "r"(x12), "r"(x13), "r"(x17)
                     : "memory");
    return x10;
}

static int set_errno_from_ret(long r) {
    if (r < 0 && r >= -4095) {
        *__errno_location() = (int)-r;
        return -1;
    }
    return 0;
}

static int valid_signal_number(int signum) {
    return signum > 0 && signum <= 64;
}

sighandler_t signal(int signum, sighandler_t handler) {
    struct kernel_sigaction act;
    struct kernel_sigaction old;
    long r;

    if (!valid_signal_number(signum) || signum == SIGKILL || signum == SIGSTOP) {
        *__errno_location() = EINVAL;
        return SIG_ERR;
    }

    act.handler = handler;
    act.flags = SA_RESTART | SA_RESTORER;
    act.restorer = __ltp_rt_restore;
    act.mask = 0;

    r = raw_syscall4(
        SYS_rt_sigaction,
        (long)signum,
        (long)(uintptr_t)&act,
        (long)(uintptr_t)&old,
        (long)sizeof(sigset_t)
    );
    if (set_errno_from_ret(r) < 0) {
        return SIG_ERR;
    }
    return old.handler;
}

int sighold(int signum) {
    sigset_t set;
    long r;

    if (!valid_signal_number(signum)) {
        *__errno_location() = EINVAL;
        return -1;
    }

    set = 1UL << (signum - 1);
    r = raw_syscall4(
        SYS_rt_sigprocmask,
        SIG_BLOCK,
        (long)(uintptr_t)&set,
        0,
        (long)sizeof(sigset_t)
    );
    return set_errno_from_ret(r);
}

int sigrelse(int signum) {
    sigset_t set;
    long r;

    if (!valid_signal_number(signum)) {
        *__errno_location() = EINVAL;
        return -1;
    }

    set = 1UL << (signum - 1);
    r = raw_syscall4(
        SYS_rt_sigprocmask,
        SIG_UNBLOCK,
        (long)(uintptr_t)&set,
        0,
        (long)sizeof(sigset_t)
    );
    return set_errno_from_ret(r);
}

int sigtimedwait(const sigset_t *set, void *info, const struct timespec *timeout) {
    long r = raw_syscall4(
        SYS_rt_sigtimedwait,
        (long)(uintptr_t)set,
        (long)(uintptr_t)info,
        (long)(uintptr_t)timeout,
        (long)sizeof(sigset_t)
    );
    if (set_errno_from_ret(r) < 0) {
        return -1;
    }
    return (int)r;
}

int sigwaitinfo(const sigset_t *set, void *info) {
    return sigtimedwait(set, info, 0);
}
