/* Explicit process timing, including bootstrap, output and shutdown.
 * wait4 accounts for all child threads (user + system); no shell pipeline. */
#define _GNU_SOURCE
#include <errno.h>
#include <inttypes.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static volatile sig_atomic_t child_pid;
static volatile sig_atomic_t expired;
static void deadline(int signal_number) {
    (void)signal_number;
    expired = 1;
    if (child_pid > 0) kill(-(pid_t)child_pid, SIGKILL);
}
static uint64_t now(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC, &t)) { perror("clock_gettime"); exit(2); }
    return (uint64_t)t.tv_sec * UINT64_C(1000000000) + (uint64_t)t.tv_nsec;
}
static uint64_t cpu_ns(struct timeval t) {
    return (uint64_t)t.tv_sec * UINT64_C(1000000000) + (uint64_t)t.tv_usec * 1000;
}
int main(int argc, char **argv) {
    if (argc < 4) { fprintf(stderr, "usage: measure METRICS SECONDS IMAGE [ARGS]\n"); return 2; }
    char *end;
    unsigned long seconds = strtoul(argv[2], &end, 10);
    if (!*argv[2] || *end || !seconds || seconds > 3600) return 2;
    struct sigaction action = {0};
    action.sa_handler = deadline;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGALRM, &action, NULL)) { perror("sigaction"); return 2; }
    uint64_t start = now();
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 2; }
    if (!pid) {
        if (setpgid(0, 0)) _exit(126);
        execv(argv[3], &argv[3]);
        perror("execv");
        _exit(127);
    }
    child_pid = pid;
    /* Either side may win this race; the child also sets its own group. */
    if (setpgid(pid, pid) && errno != EACCES && errno != ESRCH) {
        kill(pid, SIGKILL);
        perror("setpgid");
        return 2;
    }
    alarm((unsigned)seconds);
    int status;
    struct rusage usage;
    pid_t waited;
    do { waited = wait4(pid, &status, 0, &usage); } while (waited < 0 && errno == EINTR);
    alarm(0);
    uint64_t wall = now() - start;
    if (waited < 0) { perror("wait4"); return 2; }
    int code = expired ? 124 : WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
    FILE *out = fopen(argv[1], "w");
    if (!out) { perror("metrics"); return 2; }
    int bad = fprintf(out, "%" PRIu64 "\t%" PRIu64 "\t%d\n", wall,
                      cpu_ns(usage.ru_utime) + cpu_ns(usage.ru_stime), code) < 0;
    if (fclose(out)) bad = 1;
    return bad ? 2 : 0;
}
