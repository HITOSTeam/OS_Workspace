typedef long ssize_t;
typedef unsigned long uintptr_t;

#define SYS_sendmsg 211

extern int *__errno_location(void);

static inline long raw_syscall3(long n, long a0, long a1, long a2) {
    register long x10 __asm__("a0") = a0;
    register long x11 __asm__("a1") = a1;
    register long x12 __asm__("a2") = a2;
    register long x17 __asm__("a7") = n;
    __asm__ volatile("ecall"
                     : "+r"(x10)
                     : "r"(x11), "r"(x12), "r"(x17)
                     : "memory");
    return x10;
}

ssize_t sendmsg(int fd, const void *msg, int flags) {
    long r = raw_syscall3(SYS_sendmsg, (long)fd, (long)(uintptr_t)msg, (long)flags);
    if (r < 0 && r >= -4095) {
        *__errno_location() = (int)-r;
        return -1;
    }
    return (ssize_t)r;
}
