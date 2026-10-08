/* Host oracle for stop_signals.wf. Every request follows a byte from the
 * program, never a delay guessed from process startup. No WF/runtime symbols
 * are linked into this driver. */
#if !defined(_WIN32) && !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#if defined(_WIN32)
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
typedef HANDLE endpoint;
static HANDLE process;
static DWORD child_id;
#else
#include <errno.h>
#include <poll.h>
#include <signal.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
typedef int endpoint;
static pid_t process;
#endif
static endpoint input, output, errors;

static void fail(const char *reason) {
    fprintf(stderr, "stop signal host oracle: %s\n", reason);
#if defined(_WIN32)
    if (process != NULL) {
        TerminateProcess(process, 99);
        WaitForSingleObject(process, 5000);
        CloseHandle(process);
    }
#else
    if (process > 0) {
        kill(process, SIGKILL);
        waitpid(process, NULL, 0);
    }
#endif
    exit(1);
}

static void require(int condition, const char *reason) {
    if (!condition) fail(reason);
}

#if defined(_WIN32)
static BOOL WINAPI ignore_control(DWORD event) {
    return event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT;
}

static void start(const char *path) {
    SECURITY_ATTRIBUTES security = {sizeof(security), NULL, TRUE};
    HANDLE child_input, child_output, child_errors;
    require(CreatePipe(&child_input, &input, &security, 0), "stdin pipe");
    require(CreatePipe(&output, &child_output, &security, 0), "stdout pipe");
    require(CreatePipe(&errors, &child_errors, &security, 0), "stderr pipe");
    require(SetHandleInformation(input, HANDLE_FLAG_INHERIT, 0), "stdin inheritance");
    require(SetHandleInformation(output, HANDLE_FLAG_INHERIT, 0), "stdout inheritance");
    require(SetHandleInformation(errors, HANDLE_FLAG_INHERIT, 0), "stderr inheritance");
    STARTUPINFOA startup;
    PROCESS_INFORMATION child;
    memset(&startup, 0, sizeof(startup));
    memset(&child, 0, sizeof(child));
    startup.cb = sizeof(startup);
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = child_input;
    startup.hStdOutput = child_output;
    startup.hStdError = child_errors;
    char command[32768];
    int length = snprintf(command, sizeof(command), "\"%s\"", path);
    require(length > 0 && (size_t)length < sizeof(command), "child path length");
    require(CreateProcessA(NULL, command, NULL, NULL, TRUE, CREATE_NEW_PROCESS_GROUP,
                           NULL, NULL, &startup, &child), "CreateProcess new group");
    process = child.hProcess;
    child_id = child.dwProcessId;
    CloseHandle(child.hThread);
    CloseHandle(child_input);
    CloseHandle(child_output);
    CloseHandle(child_errors);
}

static int read_byte(endpoint pipe) {
    ULONGLONG until = GetTickCount64() + 30000;
    for (;;) {
        DWORD available, got;
        if (!PeekNamedPipe(pipe, NULL, 0, NULL, &available, NULL)) {
            if (GetLastError() == ERROR_BROKEN_PIPE) return -1;
            fail("PeekNamedPipe");
        }
        if (available != 0) {
            unsigned char byte;
            require(ReadFile(pipe, &byte, 1, &got, NULL) && got == 1, "ReadFile byte");
            return byte;
        }
        require(GetTickCount64() < until, "byte deadline");
        Sleep(1);
    }
}

static void gate(unsigned char byte) {
    DWORD sent;
    require(WriteFile(input, &byte, 1, &sent, NULL) && sent == 1, "WriteFile gate");
}

static void request(int interrupt) {
    /* CTRL_C cannot target one process group: this driver owns an isolated
     * console with just its child and handles the broadcast itself. */
    require(GenerateConsoleCtrlEvent(interrupt ? CTRL_C_EVENT : CTRL_BREAK_EVENT,
                                     interrupt ? 0 : child_id), "GenerateConsoleCtrlEvent");
}

static void finish(const char *expected, int signal_exit, int interrupt) {
    (void)interrupt;
    require(WaitForSingleObject(process, 30000) == WAIT_OBJECT_0, "child exit deadline");
    DWORD status;
    require(GetExitCodeProcess(process, &status), "child exit status");
    require(status == (signal_exit ? 0xc000013au : 0u), "child exit value");
    for (const char *next = expected; *next; ++next)
        require(read_byte(output) == *next, "stdout byte");
    require(read_byte(output) == -1, "stdout exact length");
    require(read_byte(errors) == -1, "unexpected stderr");
    CloseHandle(input);
    CloseHandle(output);
    CloseHandle(errors);
    CloseHandle(process);
    process = NULL;
}
#else
static void start(const char *path) {
    int in[2], out[2], err[2];
    require(pipe(in) == 0 && pipe(out) == 0 && pipe(err) == 0, "pipes");
    process = fork();
    require(process >= 0, "fork");
    if (process == 0) {
        if (dup2(in[0], STDIN_FILENO) < 0 || dup2(out[1], STDOUT_FILENO) < 0
            || dup2(err[1], STDERR_FILENO) < 0) _Exit(97);
        close(in[0]); close(in[1]); close(out[0]); close(out[1]); close(err[0]); close(err[1]);
        execl(path, path, (char *)NULL);
        _Exit(98);
    }
    close(in[0]); close(out[1]); close(err[1]);
    input = in[1]; output = out[0]; errors = err[0];
}

static int read_byte(endpoint pipe) {
    struct pollfd watch = {pipe, POLLIN, 0};
    int ready;
    do { ready = poll(&watch, 1, 30000); } while (ready < 0 && errno == EINTR);
    require(ready == 1, "byte deadline");
    unsigned char byte;
    ssize_t got;
    do { got = read(pipe, &byte, 1); } while (got < 0 && errno == EINTR);
    require(got >= 0, "read byte");
    return got == 1 ? byte : -1;
}

static void gate(unsigned char byte) {
    require(write(input, &byte, 1) == 1, "write gate");
}

static void request(int interrupt) {
    require(kill(process, interrupt ? SIGINT : SIGTERM) == 0, "kill stop request");
}

static void finish(const char *expected, int signal_exit, int interrupt) {
    for (const char *next = expected; *next; ++next)
        require(read_byte(output) == *next, "stdout byte");
    require(read_byte(output) == -1, "stdout exact length");
    require(read_byte(errors) == -1, "unexpected stderr");
    int status;
    require(waitpid(process, &status, 0) == process, "waitpid");
    process = 0;
    if (signal_exit) require(WIFSIGNALED(status) && WTERMSIG(status) ==
                             (interrupt ? SIGINT : SIGTERM), "default signal status");
    else require(WIFEXITED(status) && WEXITSTATUS(status) == 0, "orderly exit status");
    close(input); close(output); close(errors);
}
#endif

static void checkpoint(unsigned char expected) {
    require(read_byte(errors) == expected, "program checkpoint");
}

int main(int argc, char **argv) {
    require(argc == 2 || argc == 3, "compiled program path and optional lifecycle mode required");
#if defined(_WIN32)
    /* CI need not give the test process a console. Isolate broadcasts from
     * the runner and other test processes, including concurrent drivers. */
    FreeConsole();
    require(AllocConsole(), "AllocConsole");
    require(SetConsoleCtrlHandler(ignore_control, TRUE), "driver console handler");
#endif
    if (argc == 3) {
        require(strcmp(argv[2], "lifecycle") == 0, "lifecycle mode");
        start(argv[1]);
        finish("", 0, 0);
        return 0;
    }
    for (int interrupt = 0; interrupt < 2; ++interrupt) {
        start(argv[1]);
        checkpoint('B');
        request(interrupt);
        finish("", 1, interrupt);
    }
    for (int early = 0; early < 2; ++early) {
        start(argv[1]);
        checkpoint('B');
        gate('1');
        checkpoint('L');
        if (early) request(0);
        gate('G');
        if (!early) request(0);
        checkpoint('D');
        finish("T", 0, 0);
    }
    start(argv[1]);
    checkpoint('B');
    gate('2');
    checkpoint('L');
    gate('G');
    request(0);
    checkpoint('D');
    request(1);
    finish("TI", 0, 0);
    start(argv[1]);
    checkpoint('B');
    gate('C');
    checkpoint('L');
    gate('G');
    request(0);
    checkpoint('D');
    checkpoint('C');
    request(0);
    finish("T", 1, 0);
    return 0;
}
