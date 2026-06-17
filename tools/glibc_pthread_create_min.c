#define _GNU_SOURCE
#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static void *thread_main(void *arg)
{
    int *value = (int *)arg;
    printf("child: tid=%ld value=%d\n", (long)gettid(), *value);
    *value = 42;
    return arg;
}

int main(void)
{
    pthread_t th;
    int value = 7;
    int ret = pthread_create(&th, NULL, thread_main, &value);
    if (ret != 0) {
        fprintf(stderr, "pthread_create failed: %d (%s), errno=%d (%s)\n",
                ret, strerror(ret), errno, strerror(errno));
        return 1;
    }
    void *joined = NULL;
    ret = pthread_join(th, &joined);
    if (ret != 0) {
        fprintf(stderr, "pthread_join failed: %d (%s)\n", ret, strerror(ret));
        return 2;
    }
    printf("parent: joined=%p value=%d\n", joined, value);
    return value == 42 ? 0 : 3;
}
