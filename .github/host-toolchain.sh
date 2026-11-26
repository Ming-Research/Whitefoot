#!/bin/sh
# The LLVM toolchain a gate job's tests run with, installed while the job
# compiles.
#
#   sh .github/host-toolchain.sh start       begin the install and return
#   sh .github/host-toolchain.sh wait        wait for it, then check the toolchain
#   sh .github/host-toolchain.sh --self-test
#
# The Linux host takes the LLVM major .github/llvm-major pins from apt.llvm.org
# rather than Ubuntu's older clang, and makes it the /usr/bin/clang and ld.lld
# the compiler runs, so the gate builds with the toolchain releases are built
# with, including the dispatch parts' preserve_none convention, which LLVM 19
# added (compiler/verification). A cached build links its ThinLTO fragments
# with LLD on Linux; the macOS toolchain's own linker does ThinLTO itself, so
# `start` installs nothing there and `wait` only checks what the image holds.
#
# Compiling the compiler needs neither clang nor lld, only running its programs
# does, so the job starts the install, compiles, and the stage waits for the
# install where the tests begin (the Makefile's TOOLCHAIN_READY). The install's
# output is kept in a file and shown when `wait` returns, whatever the result.
# On Linux a `wait` with no `start` fails: the clang the image holds is not the
# one .github/llvm-major pins.
#
# Environment: HOST_TOOLCHAIN_DIR (the log and the status file; default under
# RUNNER_TEMP), HOST_TOOLCHAIN_LIMIT (seconds `wait` waits for the install;
# default 600), and, for the self-test only, HOST_TOOLCHAIN_INSTALL and
# HOST_TOOLCHAIN_CHECK (the commands that stand in for the install and the
# check).
set -u

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
dir=${HOST_TOOLCHAIN_DIR:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/host-toolchain}
limit=${HOST_TOOLCHAIN_LIMIT:-600}

# The commands that install LLVM and make it /usr/bin/clang and ld.lld.
install_command() {
    version=$(tr -d '[:space:]' < "$here/llvm-major")
    printf '%s' "curl -fsSL https://apt.llvm.org/llvm.sh -o '$dir/llvm.sh' \
&& sudo bash '$dir/llvm.sh' '$version' \
&& sudo ln -sf '/usr/bin/clang-$version' /usr/bin/clang \
&& sudo ln -sf '/usr/bin/ld.lld-$version' /usr/bin/ld.lld"
}

# What a job needs of the host: clang, and LLD on Linux.
check_command() {
    if [ "${RUNNER_OS:-}" = Linux ]; then
        printf '%s' 'ld.lld --version && test -x /usr/bin/clang && /usr/bin/clang --version'
    else
        printf '%s' 'test -x /usr/bin/clang && /usr/bin/clang --version'
    fi
}

start() {
    [ "${RUNNER_OS:-}" = Linux ] || return 0
    install=${HOST_TOOLCHAIN_INSTALL:-$(install_command)} || return 1
    mkdir -p "$dir" || return 1
    rm -f "$dir/status" "$dir/status.tmp"
    : > "$dir/log"
    # The status file appears, whole, only once the install has ended.
    nohup sh -c '
        sh -c "$1"
        code=$?
        echo "$code" > "$2.tmp"
        mv "$2.tmp" "$2"
    ' sh "$install" "$dir/status" > "$dir/log" 2>&1 < /dev/null &
    echo "host toolchain: install started in the background"
}

wait_for_install() {
    # No log means no install was started: on Linux the LLVM the job needs is
    # not there to wait for, and whatever clang the image holds is not the one
    # .github/llvm-major pins; elsewhere `start` installs nothing.
    if [ ! -f "$dir/log" ]; then
        if [ "${RUNNER_OS:-}" = Linux ]; then
            echo "host toolchain: the install was not started (run \`start\` first)" >&2
            return 1
        fi
        return 0
    fi
    waited=0
    while [ ! -f "$dir/status" ]; do
        if [ "$waited" -ge "$((limit * 5))" ]; then
            echo "host toolchain: the install did not end in $limit s" >&2
            show_log
            return 1
        fi
        sleep 0.2
        waited=$((waited + 1))
    done
    status=$(cat "$dir/status")
    show_log
    if [ "$status" != 0 ]; then
        echo "host toolchain: the install ended with status $status" >&2
        return 1
    fi
}

show_log() {
    echo "::group::host toolchain install log"
    cat "$dir/log"
    echo "::endgroup::"
}

wait_and_check() {
    wait_for_install || return 1
    sh -c "${HOST_TOOLCHAIN_CHECK:-$(check_command)}" || {
        echo "host toolchain: the check failed" >&2
        return 1
    }
}

self_test() {
    work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-host-toolchain.XXXXXX") || return 1
    trap 'rm -rf "$work"' EXIT
    me=$here/host-toolchain.sh
    n=0
    # run_case NAME EXPECT RUNNER_OS INSTALL CHECK LIMIT: start, then wait; EXPECT
    # is ok or fail, and the output is in $work/out.
    run_case() {
        n=$((n + 1))
        rm -rf "$work/d$n"
        HOST_TOOLCHAIN_DIR=$work/d$n
        HOST_TOOLCHAIN_INSTALL=$4
        HOST_TOOLCHAIN_CHECK=$5
        HOST_TOOLCHAIN_LIMIT=$6
        RUNNER_OS=$3
        export HOST_TOOLCHAIN_DIR HOST_TOOLCHAIN_INSTALL HOST_TOOLCHAIN_CHECK HOST_TOOLCHAIN_LIMIT RUNNER_OS
        sh "$me" start > "$work/out" 2>&1 || { echo "self-test: $1: start failed" >&2; return 1; }
        sh "$me" wait >> "$work/out" 2>&1
        code=$?
        if { [ "$2" = ok ] && [ "$code" != 0 ]; } || { [ "$2" = fail ] && [ "$code" = 0 ]; }; then
            echo "self-test: $1: wanted $2, wait returned $code" >&2
            cat "$work/out" >&2
            return 1
        fi
    }
    # The install is waited for and its output is shown.
    run_case 'a finished install passes' ok Linux 'echo installed-marker' true 5 || return 1
    grep -q installed-marker "$work/out" || { echo 'self-test: the install output is not shown' >&2; return 1; }
    # An install that fails fails the wait, whatever the check says.
    run_case 'a failed install fails' fail Linux 'echo broken; exit 3' true 5 || return 1
    grep -q 'status 3' "$work/out" || { echo 'self-test: the failed install is not named' >&2; return 1; }
    # An install that outlasts the limit fails the wait.
    run_case 'a slow install fails' fail Linux 'sleep 3' true 1 || return 1
    grep -q 'did not end' "$work/out" || { echo 'self-test: the timeout is not named' >&2; return 1; }
    # A finished install with a toolchain that fails the check fails.
    run_case 'a failed check fails' fail Linux 'true' false 5 || return 1
    # Where nothing is installed the check alone decides.
    run_case 'macOS installs nothing and checks' ok macOS 'exit 9' true 5 || return 1
    [ ! -e "$work/d$n/log" ] || { echo 'self-test: macOS started an install' >&2; return 1; }
    run_case 'macOS checks too' fail macOS 'exit 9' false 5 || return 1
    # `wait` without a `start`: on Linux it fails whatever the image holds, on
    # macOS, where nothing is installed, the check alone decides.
    HOST_TOOLCHAIN_DIR=$work/never-started
    HOST_TOOLCHAIN_INSTALL='exit 9'
    HOST_TOOLCHAIN_CHECK=true
    HOST_TOOLCHAIN_LIMIT=5
    RUNNER_OS=Linux
    export HOST_TOOLCHAIN_DIR HOST_TOOLCHAIN_INSTALL HOST_TOOLCHAIN_CHECK HOST_TOOLCHAIN_LIMIT RUNNER_OS
    if sh "$me" wait > "$work/out" 2>&1; then
        echo 'self-test: a Linux wait without a start passed' >&2
        return 1
    fi
    grep -q 'not started' "$work/out" || { echo 'self-test: the missing start is not named' >&2; return 1; }
    RUNNER_OS=macOS
    export RUNNER_OS
    sh "$me" wait > "$work/out" 2>&1 || { echo 'self-test: a macOS wait without a start failed' >&2; cat "$work/out" >&2; return 1; }
    # `start` returns while the install still runs, and the wait then outlasts it.
    HOST_TOOLCHAIN_DIR=$work/background
    HOST_TOOLCHAIN_INSTALL='sleep 1'
    HOST_TOOLCHAIN_CHECK=true
    HOST_TOOLCHAIN_LIMIT=5
    RUNNER_OS=Linux
    export HOST_TOOLCHAIN_DIR HOST_TOOLCHAIN_INSTALL HOST_TOOLCHAIN_CHECK HOST_TOOLCHAIN_LIMIT RUNNER_OS
    sh "$me" start > /dev/null 2>&1 || { echo 'self-test: the background start failed' >&2; return 1; }
    [ ! -f "$work/background/status" ] || { echo 'self-test: start waited for the install' >&2; return 1; }
    sh "$me" wait > /dev/null 2>&1 || { echo 'self-test: the background wait failed' >&2; return 1; }
    [ -f "$work/background/status" ] || { echo 'self-test: wait returned before the install ended' >&2; return 1; }
    echo "host-toolchain self-test: ok"
}

case "${1:-}" in
    start) start ;;
    wait) wait_and_check ;;
    --self-test) self_test ;;
    *) echo "usage: host-toolchain.sh start | wait | --self-test" >&2; exit 2 ;;
esac
