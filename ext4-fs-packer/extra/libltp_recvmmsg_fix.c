typedef unsigned long uintptr_t;

#define SYS_recvmmsg 243

extern int *__errno_location(void);

static inline long raw_syscall5(long n, long a0, long a1, long a2, long a3, long a4) {
    register long x10 __asm__("a0") = a0;
    register long x11 __asm__("a1") = a1;
    register long x12 __asm__("a2") = a2;
    register long x13 __asm__("a3") = a3;
    register long x14 __asm__("a4") = a4;
    register long x17 __asm__("a7") = n;
    __asm__ volatile("ecall"
                     : "+r"(x10)
                     : "r"(x11), "r"(x12), "r"(x13), "r"(x14), "r"(x17)
                     : "memory");
    return x10;
}

int recvmmsg(int fd, void *msgvec, unsigned int vlen, int flags, void *timeout) {
    long r = raw_syscall5(
        SYS_recvmmsg,
        (long)fd,
        (long)(uintptr_t)msgvec,
        (long)vlen,
        (long)flags,
        (long)(uintptr_t)timeout
    );
    if (r < 0 && r >= -4095) {
        *__errno_location() = (int)-r;
        return -1;
    }
    return (int)r;
}
