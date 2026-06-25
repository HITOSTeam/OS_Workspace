typedef unsigned long size_t;
typedef unsigned long uintptr_t;
typedef long ssize_t;

#define AT_FDCWD (-100)
#define EINVAL 22
#define SYS_readlinkat 78

extern int *__errno_location(void);

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

static ssize_t errno_or_ret(long r) {
    if (r < 0 && r >= -4095) {
        *__errno_location() = (int)-r;
        return -1;
    }
    return (ssize_t)r;
}

ssize_t readlinkat(int dirfd, const char *pathname, char *buf, size_t bufsiz) {
    if (bufsiz == 0) {
        *__errno_location() = EINVAL;
        return -1;
    }

    return errno_or_ret(raw_syscall4(
        SYS_readlinkat,
        (long)dirfd,
        (long)(uintptr_t)pathname,
        (long)(uintptr_t)buf,
        (long)bufsiz
    ));
}

ssize_t readlink(const char *pathname, char *buf, size_t bufsiz) {
    return readlinkat(AT_FDCWD, pathname, buf, bufsiz);
}
