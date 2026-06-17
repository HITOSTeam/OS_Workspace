typedef unsigned long uintptr_t;
typedef long intptr_t;

#define SYS_brk 214
#define ENOMEM 12

extern int *__errno_location(void);

static inline long raw_syscall1(long n, long a0) {
    register long x10 __asm__("a0") = a0;
    register long x17 __asm__("a7") = n;
    __asm__ volatile("ecall" : "+r"(x10) : "r"(x17) : "memory");
    return x10;
}

static uintptr_t cur_brk;

static int init_cur_brk(void) {
    if (cur_brk != 0) {
        return 0;
    }
    long r = raw_syscall1(SYS_brk, 0);
    if (r < 0 && r >= -4095) {
        *__errno_location() = (int)-r;
        return -1;
    }
    cur_brk = (uintptr_t)r;
    return 0;
}

int brk(void *addr) {
    if (init_cur_brk() < 0) {
        return -1;
    }
    uintptr_t next = (uintptr_t)addr;
    long r = raw_syscall1(SYS_brk, (long)next);
    if (r < 0 && r >= -4095) {
        *__errno_location() = (int)-r;
        return -1;
    }
    if ((uintptr_t)r != next) {
        *__errno_location() = ENOMEM;
        return -1;
    }
    cur_brk = next;
    return 0;
}

void *sbrk(intptr_t increment) {
    if (init_cur_brk() < 0) {
        return (void *)-1;
    }
    if (increment == 0) {
        return (void *)cur_brk;
    }

    uintptr_t old = cur_brk;
    uintptr_t next = old + (uintptr_t)increment;
    if ((increment > 0 && next < old) || (increment < 0 && next > old)) {
        *__errno_location() = ENOMEM;
        return (void *)-1;
    }

    if (brk((void *)next) == -1) {
        return (void *)-1;
    }
    return (void *)old;
}
