extern int *__errno_location(void);

int clone(int (*fn)(void *), void *child_stack, int flags, void *arg, ...)
{
    (void)fn;
    (void)flags;
    (void)arg;
    if (!child_stack) {
        *__errno_location() = 22;
        return -1;
    }

    *__errno_location() = 38;
    return -1;
}
