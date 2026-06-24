#include <errno.h>
#include <sched.h>
#include <sys/types.h>

enum {
    NR_SCHED_SETPARAM = 118,
    NR_SCHED_SETSCHEDULER = 119,
    NR_SCHED_GETSCHEDULER = 120,
    NR_SCHED_GETPARAM = 121,
};

static long syscall1(long number, long arg0)
{
    register long a0 __asm__("$a0") = arg0;
    register long a7 __asm__("$a7") = number;
    __asm__ volatile("syscall 0" : "+r"(a0) : "r"(a7) : "memory");
    return a0;
}

static long syscall2(long number, long arg0, long arg1)
{
    register long a0 __asm__("$a0") = arg0;
    register long a1 __asm__("$a1") = arg1;
    register long a7 __asm__("$a7") = number;
    __asm__ volatile("syscall 0" : "+r"(a0) : "r"(a1), "r"(a7) : "memory");
    return a0;
}

static long syscall3(long number, long arg0, long arg1, long arg2)
{
    register long a0 __asm__("$a0") = arg0;
    register long a1 __asm__("$a1") = arg1;
    register long a2 __asm__("$a2") = arg2;
    register long a7 __asm__("$a7") = number;
    __asm__ volatile("syscall 0" : "+r"(a0) : "r"(a1), "r"(a2), "r"(a7) : "memory");
    return a0;
}

static int syscall_result(long result)
{
    if (result < 0) {
        errno = (int)-result;
        return -1;
    }
    return (int)result;
}

int sched_getparam(pid_t pid, struct sched_param *param)
{
    return syscall_result(syscall2(NR_SCHED_GETPARAM, pid, (long)param));
}

int sched_getscheduler(pid_t pid)
{
    return syscall_result(syscall1(NR_SCHED_GETSCHEDULER, pid));
}

int sched_setparam(pid_t pid, const struct sched_param *param)
{
    return syscall_result(syscall2(NR_SCHED_SETPARAM, pid, (long)param));
}

int sched_setscheduler(pid_t pid, int policy, const struct sched_param *param)
{
    return syscall_result(syscall3(NR_SCHED_SETSCHEDULER, pid, policy, (long)param));
}
