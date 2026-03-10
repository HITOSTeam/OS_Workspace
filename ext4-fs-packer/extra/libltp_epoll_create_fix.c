typedef long intptr_t;

#define SYS_epoll_create1 20
#define EINVAL 22

extern int *__errno_location(void);

static inline long raw_syscall1(long n, long a0) {
    register long x10 __asm__("a0") = a0;
    register long x17 __asm__("a7") = n;
    __asm__ volatile("ecall" : "+r"(x10) : "r"(x17) : "memory");
    return x10;
}

int epoll_create(int size) {
    if (size <= 0) {
        *__errno_location() = EINVAL;
        return -1;
    }

    long r = raw_syscall1(SYS_epoll_create1, 0);
    if (r < 0 && r >= -4095) {
        *__errno_location() = (int)-r;
        return -1;
    }
    return (int)r;
}
