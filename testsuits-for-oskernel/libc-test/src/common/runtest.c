#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <signal.h>
#include <time.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <sys/time.h>
#include <sys/resource.h>
#include <unistd.h>
#include "test.h"

static void handler(int s)
{
}

static int start(char *wrap, char *argv[])
{
	int pid;

	pid = fork();
	if (pid == 0) {
		t_setrlim(RLIMIT_STACK, 8*1024*1024);
		if (*wrap) {
			argv--;
			argv[0] = wrap;
		}
		execv(argv[0], argv);
		t_error("%s exec failed: %s\n", argv[0], strerror(errno));
		exit(1);
	}
	return pid;
}

static void usage(char *argv[])
{
	t_error("usage: %s [-t timeoutsec] [-w wrapcmd] cmd [args..]\n", argv[0]);
	exit(-1);
}

static long long now_ms(void)
{
	struct timeval tv;
	gettimeofday(&tv, 0);
	return (long long)tv.tv_sec * 1000 + tv.tv_usec / 1000;
}

int main(int argc, char *argv[])
{
	char *wrap = "";
	int timeoutsec = 5;
	int timeout = 0;
	int status;
	int reaped = 0;
	int opt;
	int pid;

	while ((opt = getopt(argc, argv, "w:t:")) != -1) {
		switch (opt) {
		case 'w':
			wrap = optarg;
			break;
		case 't':
			timeoutsec = atoi(optarg);
			break;
		default:
			usage(argv);
		}
	}
	if (optind >= argc)
		usage(argv);
	argv += optind;
	t_printf("========== START %s %s ==========\n", wrap, argv[0]);
	pid = start(wrap, argv);
	int err = 0;
	if (pid == -1) {
		t_error("%s fork failed: %s\n", argv[0], strerror(errno));
		t_printf("FAIL %s [internal]\n", argv[0]);
		err = 1;
	}
	if (!err) {
		long long deadline = now_ms() + (long long)timeoutsec * 1000;
		for (;;) {
			int r = waitpid(pid, &status, WNOHANG);
			if (r == pid)
			{
				reaped = 1;
				break;
			}
			if (r == 0) {
				if (now_ms() >= deadline) {
					timeout = 1;
					if (kill(pid, SIGKILL) == -1) {
						t_error("%s kill failed: %s\n", argv[0], strerror(errno));
						err = 1;
					}
					(void)waitpid(pid, &status, 0);
					reaped = 1;
					break;
				}
				struct timespec ts = {0, 10 * 1000 * 1000};
				nanosleep(&ts, 0);
				continue;
			}
			t_error("%s waitpid failed: %s\n", argv[0], strerror(errno));
			t_printf("FAIL %s [internal]\n", argv[0]);
			err = 1;
			break;
		}
	}
	if (!reaped && waitpid(pid, &status, 0) != pid) {
		t_error("%s waitpid failed: %s\n", argv[0], strerror(errno));
		t_printf("FAIL %s [internal]\n", argv[0]);
		err = 1;
	}
	if (WIFEXITED(status)) {
		if (WEXITSTATUS(status) != 0) {
			t_printf("FAIL %s [status %d]\n", argv[0], WEXITSTATUS(status));
			err = 1;
		}
	} else if (timeout) {
		t_printf("FAIL %s [timed out]\n", argv[0]);
		err = 1;
	} else if (WIFSIGNALED(status)) {
		t_printf("FAIL %s [signal %s]\n", argv[0], strsignal(WTERMSIG(status)));
		err = 1;
	} else {
		t_printf("FAIL %s [unknown]\n", argv[0]);
		err = 1;
	}


	if (err == 0) {
		t_printf("Pass!\n");
	}
	t_printf("========== END %s %s ==========\n", wrap, argv[0]);
	return 1;
}
