#!/bin/sh
# Exercise the gate wrapper's ownership and exit-status boundaries cheaply.
#
# The cases run in groups, and the groups run side by side. Each group has a
# directory of its own with its own lock in it, so no group takes another's
# lock, reads its record or counts its processes. The cases of one group run in
# order, as some read what an earlier one wrote. The test takes about as long as
# its slowest group, and that time is mostly the wrapper's own waiting: the 1 s
# limit, the 0.5 s it waits before it fails a command that left a process
# behind, and its poll every 0.1 s. A case waits for the event it stands for,
# never for a fixed time.
set -eu
runner=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/run-check.pl
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-check-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
unset WHITEFOOT_CHECK_OWNER WHITEFOOT_TIME_BUDGET_RECORD WHITEFOOT_TIME_BUDGET_FILE WHITEFOOT_CPUINFO

# Wait until a command in the background has written the file that marks a
# point in it. The bound only stops a hang.
appears() {
    attempt=0
    while [ ! -s "$1" ]; do
        attempt=$((attempt + 1))
        if [ "$attempt" -ge 200 ]; then
            echo "$1 was not written" >&2
            return 1
        fi
        sleep 0.01
    done
}

# Wait until the process whose id the file holds has gone. A process the
# wrapper killed is gone at once; one it left behind lives for its 30 seconds.
gone() {
    pid=$(cat "$1")
    if [ -z "$pid" ]; then
        echo "$1 holds no process id" >&2
        return 1
    fi
    attempt=0
    while kill -0 "$pid" 2>/dev/null; do
        attempt=$((attempt + 1))
        if [ "$attempt" -ge 100 ]; then
            echo "process $pid of $1 is still running" >&2
            return 1
        fi
        sleep 0.01
    done
}

# A command's own status passes through, and a nested wrapper runs inside its
# owner's lock.
group_status() {
    status=0
    perl "$runner" failure sh -c 'exit 17' > "$dir/failure.log" 2>&1 || status=$?
    test "$status" -eq 17
    test ! -e "$dir/lock"

    perl "$runner" parent perl "$runner" nested true > "$dir/nested.log" 2>&1
    test ! -e "$dir/lock"
}

# The holder keeps the lock until the competing command has been refused.
group_exclusion() {
    perl "$runner" holder sh -c \
        'echo held > "$1"; n=0; while [ ! -e "$2" ] && [ "$n" -lt 3000 ]; do n=$((n + 1)); sleep 0.01; done' \
        sh "$dir/ready" "$dir/release" > "$dir/holder.log" 2>&1 &
    holder=$!
    trap 'touch "$dir/release"; wait "$holder" 2>/dev/null || :' EXIT
    appears "$dir/ready"
    status=0
    perl "$runner" competing true > "$dir/competing.log" 2>&1 || status=$?
    test "$status" -eq 75
    grep -q 'already owned' "$dir/competing.log"
    touch "$dir/release"
    wait "$holder"
    trap - EXIT
    test ! -e "$dir/lock"
}

# Parallelism stays Cargo's and the test harness's own default unless named.
group_parallelism() {
    (
        unset CARGO_BUILD_JOBS RUST_TEST_THREADS
        perl "$runner" defaults sh -c 'test -z "${CARGO_BUILD_JOBS+set}${RUST_TEST_THREADS+set}"'
    ) > "$dir/defaults.log" 2>&1
    (
        CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1
        export CARGO_BUILD_JOBS RUST_TEST_THREADS
        perl "$runner" chosen sh -c 'test "$CARGO_BUILD_JOBS:$RUST_TEST_THREADS" = 1:1'
    ) > "$dir/chosen.log" 2>&1
    test ! -e "$dir/lock"
}

# A command past its limit is stopped with its whole group. The limit is the
# shortest the wrapper takes, 1 s, and there is no grace before KILL: a group
# that honours TERM is gone either way, and the grace has the next case.
group_timeout() {
    status=0
    WHITEFOOT_CHECK_TIMEOUT=1 WHITEFOOT_CHECK_KILL_AFTER=0 perl "$runner" timeout sh -c \
        'sleep 30 & echo $! > "$1"; wait' sh "$dir/child" \
        > "$dir/timeout.log" 2>&1 || status=$?
    test "$status" -eq 124
    test ! -e "$dir/lock"
    gone "$dir/child"
}

# A group that ignores TERM is killed once its grace has passed: at once with
# none, where the default grace would take 2 seconds more.
group_grace() {
    status=0
    started=$(perl -MTime::HiRes=time -e 'printf "%.3f", time')
    WHITEFOOT_CHECK_TIMEOUT=1 WHITEFOOT_CHECK_KILL_AFTER=0 perl "$runner" grace sh -c \
        'trap "" TERM; sleep 30 & echo $! > "$1"; wait' sh "$dir/grace-child" \
        > "$dir/grace.log" 2>&1 || status=$?
    test "$status" -eq 124
    perl -MTime::HiRes=time -e 'exit(time - $ARGV[0] < 2.5 ? 0 : 1)' "$started"
    test ! -e "$dir/lock"
    gone "$dir/grace-child"
}

# A TERM to the owner stops its group, the nested wrapper in it as well.
group_signal() {
    WHITEFOOT_CHECK_KILL_AFTER=0 perl "$runner" signal perl "$runner" nested sh -c \
        'sleep 30 & echo $! > "$1"; wait' sh "$dir/signal-child" \
        > "$dir/signal.log" 2>&1 &
    owner=$!
    trap 'kill -TERM "$owner" 2>/dev/null || :; wait "$owner" 2>/dev/null || :' EXIT
    appears "$dir/signal-child"
    kill -TERM "$owner"
    status=0
    wait "$owner" || status=$?
    trap - EXIT
    test "$status" -eq 143
    test ! -e "$dir/lock"
    gone "$dir/signal-child"
}

# A command that leaves a process behind fails, and the process is stopped.
group_orphan() {
    status=0
    WHITEFOOT_CHECK_KILL_AFTER=0 perl "$runner" orphan sh -c 'sleep 30 & echo $! > "$1"' sh "$dir/orphan" \
        > "$dir/orphan.log" 2>&1 || status=$?
    test "$status" -eq 1
    test ! -e "$dir/lock"
    gone "$dir/orphan"
}

# Emulate the platform seen by the wrapper, so every CI host checks Linux,
# macOS and Windows budget policy without changing the real command's host.
# An external command, not a shell function: a POSIX shell may keep the
# variable assignments written before a function call (bash does), which
# would leak one case's environment into the next.
cat > "$work/run-host.pl" <<'HOST'
$^O = shift @ARGV;
my $runner = shift @ARGV;
do $runner;
die $@ || $!;
HOST
cat > "$work/budgets" <<'BUDGETS'
# Different class/host values expose selection of the wrong column.
cpu fast  Test Fast CPU
cpu slow  Test Slow CPU
label fast slow macos windows
within 600 0 0 0
over 0 600 0 0
parent 600 0 0 0
nowhere - 600 - -
timeout 0 600 0 0
malformed 1.5 600 1.5 1.5
unknown-max 0 600 0 0
fallback-before-last 600 0 0 0
host-within 0 0 600 600
host-over 600 600 0 0
BUDGETS
printf 'model name\t: Test Fast CPU\n' > "$work/cpuinfo"

# The budget groups read the test's table and CPU model.
test_table() {
    WHITEFOOT_TIME_BUDGET_FILE=$work/budgets
    WHITEFOOT_CPUINFO=$work/cpuinfo
    export WHITEFOOT_TIME_BUDGET_FILE WHITEFOOT_CPUINFO
}

group_record() {
    test_table
    record=$dir/over-budget
    perl "$work/run-host.pl" linux "$runner" over true > "$dir/report.log" 2>&1
    grep -q 'OVER BUDGET: enforced: over took' "$dir/report.log"
    perl "$work/run-host.pl" linux "$runner" within true > "$dir/within.log" 2>&1
    grep -q '== BUDGET within: [0-9.]* s of 600 s (linux) ==' "$dir/within.log"
    perl "$work/run-host.pl" linux "$runner" unlisted true > "$dir/unlisted.log" 2>&1
    if grep -q 'BUDGET' "$dir/unlisted.log"; then exit 1; fi
    test ! -e "$record"

    # A record collects every overrun; nested commands keep their own statuses.
    WHITEFOOT_TIME_BUDGET_RECORD=$record perl "$work/run-host.pl" linux "$runner" parent sh -c \
        'perl -e "$3" "$1" over true; perl -e "$3" "$1" within true; touch "$2"' \
        sh "$runner" "$dir/continued" '$^O = "linux"; do shift @ARGV; die $@ || $!' \
        > "$dir/recorded.log" 2>&1
    test -f "$dir/continued"
    grep -q '^  ENFORCED: over took [0-9.]* s, over its 0 s' "$record"
    if grep -q 'within\|parent' "$record"; then exit 1; fi
    test ! -e "$dir/lock"
    status=0
    WHITEFOOT_TIME_BUDGET_RECORD=$record perl "$work/run-host.pl" linux "$runner" over sh -c 'exit 17' > "$dir/over-failed.log" 2>&1 || status=$?
    test "$status" -eq 17
    for label in unlisted nowhere; do
        WHITEFOOT_TIME_BUDGET_RECORD=$record perl "$work/run-host.pl" linux "$runner" "$label" true > "$dir/$label-recorded.log" 2>&1
        grep -q "^  ENFORCED: $label has no linux budget" "$record"
    done
    test "$(grep -c '^  ENFORCED: over took' "$record")" -eq 2
    WHITEFOOT_TIME_BUDGET_RECORD=$record perl "$work/run-host.pl" linux "$runner" over true > "$dir/over-passed.log" 2>&1
    test "$(grep -c '^  ENFORCED: over took' "$record")" -eq 3
    WHITEFOOT_TIME_BUDGET_RECORD=$record perl "$work/run-host.pl" linux "$runner" malformed true > "$dir/malformed.log" 2>&1
    grep -q "malformed needs seconds or - for fast" "$record"

    status=0
    perl "$runner" --budget-verdict "$record" > "$dir/verdict.log" 2>&1 || status=$?
    test "$status" -eq 1
    grep -q 'TIME BUDGETS EXCEEDED' "$dir/verdict.log"
    : > "$dir/empty-record"
    perl "$runner" --budget-verdict "$dir/empty-record" > "$dir/verdict-empty.log" 2>&1
    perl "$runner" --budget-verdict "$dir/absent-record" > "$dir/verdict-absent.log" 2>&1
    for arguments in "''" "'$record' extra"; do
        status=0
        eval "perl \"\$runner\" --budget-verdict $arguments > \"\$dir/verdict-usage.log\" 2>&1" || status=$?
        test "$status" -ne 0
        test "$status" -ne 1
        grep -q usage "$dir/verdict-usage.log"
    done
}

group_classes() {
    test_table
    # The second mapped model selects its own column without a fallback note.
    printf 'model name\t: Test Slow CPU\n' > "$dir/slow-cpuinfo"
    WHITEFOOT_CPUINFO=$dir/slow-cpuinfo WHITEFOOT_TIME_BUDGET_RECORD=$dir/slow-record \
        perl "$work/run-host.pl" linux "$runner" unknown-max true > "$dir/slow.log" 2>&1
    grep -q '== BUDGET unknown-max: [0-9.]* s of 600 s (linux) ==' "$dir/slow.log"
    test ! -s "$dir/slow-record"

    # Unknown models use the largest Linux class value and record a note even
    # when the selected budget is met. Notes alone never fail the verdict.
    printf 'model name\t: Test Unmapped CPU\n' > "$dir/unknown-cpuinfo"
    WHITEFOOT_CPUINFO=$dir/unknown-cpuinfo WHITEFOOT_TIME_BUDGET_RECORD=$dir/unknown-record \
        perl "$work/run-host.pl" linux "$runner" unknown-max true > "$dir/unknown.log" 2>&1
    grep -q '== BUDGET unknown-max: [0-9.]* s of 600 s (linux) ==' "$dir/unknown.log"
    grep -q '== BUDGET NOTE: Linux CPU model "Test Unmapped CPU"' "$dir/unknown.log"
    grep -q '^  NOTE: Linux CPU model "Test Unmapped CPU".*slowest Linux class budget (600 s)' "$dir/unknown-record"
    perl "$runner" --budget-verdict "$dir/unknown-record" > "$dir/unknown-verdict.log" 2>&1

    # The largest class need not be the last column, and a model extending a
    # mapped name must still use the fallback rather than a prefix match.
    WHITEFOOT_CPUINFO=$dir/unknown-cpuinfo WHITEFOOT_TIME_BUDGET_RECORD=$dir/before-last-record \
        perl "$work/run-host.pl" linux "$runner" fallback-before-last true > "$dir/before-last.log" 2>&1
    grep -q '== BUDGET fallback-before-last: [0-9.]* s of 600 s (linux) ==' "$dir/before-last.log"
    grep -q '== BUDGET NOTE: Linux CPU model "Test Unmapped CPU".*(600 s)' "$dir/before-last.log"
    grep -q '^  NOTE: Linux CPU model "Test Unmapped CPU".*(600 s)' "$dir/before-last-record"
    if grep -q '^  ENFORCED:\|^  REPORT-ONLY:' "$dir/before-last-record"; then exit 1; fi
    perl "$runner" --budget-verdict "$dir/before-last-record" > "$dir/before-last-verdict.log" 2>&1
    printf 'model name\t: Test Fast CPU Extra\n' > "$dir/near-cpuinfo"
    WHITEFOOT_CPUINFO=$dir/near-cpuinfo WHITEFOOT_TIME_BUDGET_RECORD=$dir/near-record \
        perl "$work/run-host.pl" linux "$runner" unknown-max true > "$dir/near.log" 2>&1
    grep -q '== BUDGET unknown-max: [0-9.]* s of 600 s (linux) ==' "$dir/near.log"
    grep -q '== BUDGET NOTE: Linux CPU model "Test Fast CPU Extra".*(600 s)' "$dir/near.log"
    grep -q '^  NOTE: Linux CPU model "Test Fast CPU Extra".*(600 s)' "$dir/near-record"
    if grep -q '^  ENFORCED:\|^  REPORT-ONLY:' "$dir/near-record"; then exit 1; fi
    perl "$runner" --budget-verdict "$dir/near-record" > "$dir/near-verdict.log" 2>&1

    # A known mapping without a column and unreadable cpuinfo use the same
    # fallback. A mapped '-' was checked above and must never fall back.
    { printf 'cpu omitted  Test Omitted CPU\n'; cat "$work/budgets"; } > "$dir/omitted-budgets"
    printf 'model name\t: Test Omitted CPU\n' > "$dir/omitted-cpuinfo"
    WHITEFOOT_TIME_BUDGET_FILE=$dir/omitted-budgets WHITEFOOT_CPUINFO=$dir/omitted-cpuinfo \
        WHITEFOOT_TIME_BUDGET_RECORD=$dir/omitted-record perl "$work/run-host.pl" linux "$runner" unknown-max true > "$dir/omitted.log" 2>&1
    grep -q '^  NOTE: Linux CPU model "Test Omitted CPU".*(600 s)' "$dir/omitted-record"
    WHITEFOOT_CPUINFO=$dir/no-cpuinfo WHITEFOOT_TIME_BUDGET_RECORD=$dir/no-cpuinfo-record \
        perl "$work/run-host.pl" linux "$runner" unknown-max true > "$dir/no-cpuinfo.log" 2>&1
    grep -q '^  NOTE: Linux CPU model "<unavailable>".*(600 s)' "$dir/no-cpuinfo-record"
}

# macOS overruns remain visible but do not enforce; missing rows/values and
# malformed tables still fail there.
group_macos() {
    test_table
    WHITEFOOT_TIME_BUDGET_RECORD=$dir/macos-record perl "$work/run-host.pl" darwin "$runner" over true > "$dir/macos.log" 2>&1
    grep -q '== OVER BUDGET: report-only: over took' "$dir/macos.log"
    grep -q '^  REPORT-ONLY: over took' "$dir/macos-record"
    perl "$runner" --budget-verdict "$dir/macos-record" > "$dir/macos-verdict.log" 2>&1
    grep -q 'REPORT-ONLY: over took' "$dir/macos-verdict.log"
    for label in unlisted nowhere malformed; do
        WHITEFOOT_TIME_BUDGET_RECORD=$dir/macos-missing perl "$work/run-host.pl" darwin "$runner" "$label" true > "$dir/macos-$label.log" 2>&1
    done
    grep -q '^  ENFORCED: unlisted has no macos budget' "$dir/macos-missing"
    grep -q '^  ENFORCED: nowhere has no macos budget' "$dir/macos-missing"
    grep -q '^  ENFORCED: .*malformed needs seconds or - for macos' "$dir/macos-missing"
    status=0
    perl "$runner" --budget-verdict "$dir/macos-missing" > "$dir/macos-missing-verdict.log" 2>&1 || status=$?
    test "$status" -eq 1
}

# Linux and Windows overruns enforce.
group_hosts() {
    test_table
    WHITEFOOT_TIME_BUDGET_RECORD=$dir/linux-record perl "$work/run-host.pl" linux "$runner" over true > "$dir/linux.log" 2>&1
    WHITEFOOT_TIME_BUDGET_RECORD=$dir/windows-record perl "$work/run-host.pl" MSWin32 "$runner" over true > "$dir/windows.log" 2>&1
    for platform in linux windows; do
        grep -q '^  ENFORCED: over took' "$dir/$platform-record"
        status=0
        perl "$runner" --budget-verdict "$dir/$platform-record" > "$dir/$platform-verdict.log" 2>&1 || status=$?
        test "$status" -eq 1
    done

    # Both Linux columns oppose the host's value, so selecting either Linux
    # column fails even when the macOS final verdict would still report success.
    for budget_host in macos windows; do
        case "$budget_host" in
            macos) platform=darwin; kind=REPORT-ONLY; mode=report-only; expected=0 ;;
            windows) platform=MSWin32; kind=ENFORCED; mode=enforced; expected=1 ;;
        esac
        WHITEFOOT_TIME_BUDGET_RECORD=$dir/$budget_host-within-record \
            perl "$work/run-host.pl" "$platform" "$runner" host-within true > "$dir/$budget_host-within.log" 2>&1
        grep -q "== BUDGET host-within: [0-9.]* s of 600 s ($budget_host) ==" "$dir/$budget_host-within.log"
        test ! -s "$dir/$budget_host-within-record"
        perl "$runner" --budget-verdict "$dir/$budget_host-within-record" > "$dir/$budget_host-within-verdict.log" 2>&1
        WHITEFOOT_TIME_BUDGET_RECORD=$dir/$budget_host-over-record \
            perl "$work/run-host.pl" "$platform" "$runner" host-over true > "$dir/$budget_host-over.log" 2>&1
        grep -q "== OVER BUDGET: $mode: host-over took [0-9.]* s, over its 0 s $budget_host budget ==" "$dir/$budget_host-over.log"
        grep -q "^  $kind: host-over took [0-9.]* s, over its 0 s $budget_host budget$" "$dir/$budget_host-over-record"
        if grep -q '^  NOTE:' "$dir/$budget_host-over-record"; then exit 1; fi
        status=0
        perl "$runner" --budget-verdict "$dir/$budget_host-over-record" > "$dir/$budget_host-over-verdict.log" 2>&1 || status=$?
        test "$status" -eq "$expected"
        grep -q "^  $kind: host-over took" "$dir/$budget_host-over-verdict.log"
    done
}

# WHITEFOOT_BUDGET_REPORT_ONLY reports only the labels it names, on Linux too;
# an unnamed label stays enforced, and so does a named label with no budget.
group_report_only() {
    test_table
    WHITEFOOT_BUDGET_REPORT_ONLY=over WHITEFOOT_TIME_BUDGET_RECORD=$dir/named-record \
        perl "$work/run-host.pl" linux "$runner" over true > "$dir/named.log" 2>&1
    grep -q '== OVER BUDGET: report-only: over took' "$dir/named.log"
    grep -q '^  REPORT-ONLY: over took' "$dir/named-record"
    perl "$runner" --budget-verdict "$dir/named-record" > "$dir/named-verdict.log" 2>&1
    status=0
    WHITEFOOT_BUDGET_REPORT_ONLY=other WHITEFOOT_TIME_BUDGET_RECORD=$dir/unnamed-record \
        perl "$work/run-host.pl" linux "$runner" over true > "$dir/unnamed.log" 2>&1
    grep -q '^  ENFORCED: over took' "$dir/unnamed-record"
    perl "$runner" --budget-verdict "$dir/unnamed-record" > "$dir/unnamed-verdict.log" 2>&1 || status=$?
    test "$status" -eq 1
    WHITEFOOT_BUDGET_REPORT_ONLY=unlisted WHITEFOOT_TIME_BUDGET_RECORD=$dir/nowhere-record \
        perl "$work/run-host.pl" linux "$runner" unlisted true > "$dir/nowhere.log" 2>&1
    grep -q '^  ENFORCED: unlisted has no linux budget' "$dir/nowhere-record"
}

# A cancelled stage has no budget verdict.
group_cancelled() {
    test_table
    status=0
    WHITEFOOT_CHECK_TIMEOUT=1 WHITEFOOT_CHECK_KILL_AFTER=0 WHITEFOOT_TIME_BUDGET_RECORD=$dir/timeout-record \
        perl "$work/run-host.pl" linux "$runner" timeout sleep 5 > "$dir/timeout-budget.log" 2>&1 || status=$?
    test "$status" -eq 124
    test ! -s "$dir/timeout-record"
}

group_record_file() {
    test_table
    # A record that cannot be written stops the command before it runs; the
    # relative case runs in the group's directory, where a regression would
    # leave its file.
    for bad in relative-record "$dir/no-such-directory/record"; do
        status=0
        (cd "$dir" && WHITEFOOT_TIME_BUDGET_RECORD=$bad perl "$runner" within touch "$dir/ran") \
            > "$dir/bad-record.log" 2>&1 || status=$?
        test "$status" -ne 0
        test ! -e "$dir/ran"
    done
    test ! -e "$dir/relative-record"

    # An unreadable table never changes a status; with a record it is recorded.
    status=0
    WHITEFOOT_TIME_BUDGET_FILE=$dir/missing perl "$runner" within sh -c 'exit 17' > "$dir/missing.log" 2>&1 || status=$?
    test "$status" -eq 17
    grep -q 'cannot read' "$dir/missing.log"
    WHITEFOOT_TIME_BUDGET_FILE=$dir/missing WHITEFOOT_TIME_BUDGET_RECORD=$dir/missing-record \
        perl "$runner" within true > "$dir/missing-recorded.log" 2>&1
    grep -q 'cannot read' "$dir/missing-record"
    test ! -e "$dir/lock"
}

# On Linux and macOS the wrapper itself prints the command's real, user and sys
# time in the format of `time -p`, before the END line, because Git for
# Windows has no /usr/bin/time: a command that computes for a second must show
# most of it as user time, which the wrapper's own CPU time, spent only in its
# polling loop, would not. The command computes for a second of wall time by
# its nature; the group runs beside the others, which mostly wait.
group_cpu() {
    perl "$runner" cpu perl -MTime::HiRes=time -e '$t = time; 1 while time - $t < 1' > "$dir/cpu.log" 2>&1
    awk '/^real [0-9.]+$/ { real = 1 }
         /^user [0-9.]+$/ { if ($2 >= 0.3) user = 1 }
         /^sys [0-9.]+$/ { sys = 1 }
         /^== END cpu: / { if (real && user && sys) ok = 1 }
         END { exit !ok }' "$dir/cpu.log"
    test ! -e "$dir/lock"
}

# Git for Windows' perl, which runs the wrapper in the Windows I/O host job,
# reports its host as cygwin; it reads the windows column. That perl counts no
# native descendant's CPU time, so a Windows host prints no real, user or sys
# line, and the END line gives the wall time.
group_cygwin() {
    test_table
    WHITEFOOT_TIME_BUDGET_RECORD=$dir/cygwin-record \
        perl "$work/run-host.pl" cygwin "$runner" host-over true > "$dir/cygwin.log" 2>&1
    grep -q "== OVER BUDGET: enforced: host-over took [0-9.]* s, over its 0 s windows budget ==" "$dir/cygwin.log"
    grep -q '^  ENFORCED: host-over took [0-9.]* s, over its 0 s windows budget$' "$dir/cygwin-record"
    WHITEFOOT_TIME_BUDGET_RECORD=$dir/cygwin-within-record \
        perl "$work/run-host.pl" cygwin "$runner" host-within true > "$dir/cygwin-within.log" 2>&1
    grep -q '== BUDGET host-within: [0-9.]* s of 600 s (windows) ==' "$dir/cygwin-within.log"
    # The Windows I/O job's last step fails when the record does not exist, and
    # the verdict reads a missing record as "nothing over budget": a stage
    # within its budget must still leave the record created, and empty.
    test -e "$dir/cygwin-within-record"
    test ! -s "$dir/cygwin-within-record"
    grep -q '^== END host-within: ' "$dir/cygwin-within.log"
    if grep -qE '^(real|user|sys) ' "$dir/cygwin.log" "$dir/cygwin-within.log"; then exit 1; fi
}

# A command that cannot be executed fails the stage with a nonzero status,
# says why, and leaves no lock behind: the wrapper executes the command itself
# since it no longer runs it through /usr/bin/time.
group_missing_command() {
    status=0
    perl "$runner" missing-command definitely-not-a-command-for-whitefoot > "$dir/missing-command.log" 2>&1 || status=$?
    test "$status" -ne 0
    grep -q 'execute check' "$dir/missing-command.log"
    grep -q '^== END missing-command: ' "$dir/missing-command.log"
    test ! -e "$dir/lock"
}

# The Windows I/O host job (.github/workflows/io-hosts.yml, completion-windows)
# is held to the budget table by its form: the job's default shell runs every
# step under the wrapper with the step's own id as its label, each step that is
# timed has an id, and the id is a row of the table. These groups read the
# workflow and the table as they are, so a step added without an id or without a
# row, or a row without a step, fails here and not in a Windows run.
repo=$(CDPATH= cd -- "$(dirname -- "$runner")/.." && pwd)
workflow=$repo/.github/workflows/io-hosts.yml
table=$repo/.github/time-budgets.txt

# The steps against the rows.
group_windows_form() {
    # One line per run step of the job: "timed <id>", "noid" for a step the
    # default shell would time under a generated name, or "untimed <flags>" for
    # a step that states `shell: bash`, with v (calls the verdict), a (if:
    # always()) and r (writes the record's name to GITHUB_ENV).
    awk '
    function flush() {
        if (step && run) {
            if (bash) printf "untimed %s%s%s\n", (verdict ? "v" : ""), (always ? "a" : ""), (names ? "r" : "")
            else if (id == "") print "noid"
            else print "timed " id
        }
        step = 0
    }
    /^  [a-z][a-z-]*:$/ { flush(); injob = ($1 == "completion-windows:"); next }
    !injob { next }
    /^      - / { flush(); step = 1; id = ""; run = 0; bash = 0; verdict = 0; always = 0; names = 0 }
    /^      - id: / { id = $3 }
    /^        id: / { id = $2 }
    /^        run:/ { run = 1 }
    /^        shell: bash$/ { bash = 1 }
    /^        if: always\(\)$/ { always = 1 }
    /--budget-verdict/ { verdict = 1 }
    /WHITEFOOT_TIME_BUDGET_RECORD=.*GITHUB_ENV/ { names = 1 }
    END { flush() }
    ' "$workflow" > "$dir/windows-steps"
    test -s "$dir/windows-steps"
    if grep -qx noid "$dir/windows-steps"; then exit 1; fi
    test "$(grep -c '^untimed ' "$dir/windows-steps")" -eq 2
    grep -qx 'untimed r' "$dir/windows-steps"
    grep -qx 'untimed va' "$dir/windows-steps"
    sed -n 's/^timed //p' "$dir/windows-steps" | sort > "$dir/windows-step-ids"
    test -s "$dir/windows-step-ids"
    test "$(uniq -d < "$dir/windows-step-ids" | wc -l)" -eq 0
    # The table side: the rows that name windows stages, with seconds of at
    # least the 10 s floor in the windows column and '-' in every other column.
    awk '
    /^[ \t]*#/ || /^[ \t]*$/ { next }
    $1 == "label" { nf = NF; for (i = 2; i <= NF; i++) if ($i == "windows") w = i; next }
    $1 ~ /^windows-/ {
        if (!w || NF != nf) bad = 1
        for (i = 2; i <= NF; i++) {
            if (i == w) { if ($i !~ /^[0-9]+$/ || $i + 0 < 10) bad = 1 }
            else if ($i != "-") bad = 1
        }
        print $1
    }
    END { exit bad }
    ' "$table" > "$dir/windows-rows-unsorted"
    sort "$dir/windows-rows-unsorted" > "$dir/windows-rows"
    test -s "$dir/windows-rows"
    cmp "$dir/windows-step-ids" "$dir/windows-rows"
    # compiler/build, which compiler/Makefile labels, has a row with seconds in
    # every Linux column and '-' for the hosts that do not run it.
    grep -q 'CHECK_RUN) compiler/build ' "$repo/compiler/Makefile"
    awk '
    /^[ \t]*#/ || /^[ \t]*$/ { next }
    $1 == "label" { for (i = 2; i <= NF; i++) col[i] = $i; n = NF; next }
    $1 == "compiler/build" {
        found = 1
        if (NF != n) bad = 1
        for (i = 2; i <= NF; i++) {
            if (col[i] == "macos" || col[i] == "windows") { if ($i != "-") bad = 1 }
            else if ($i !~ /^[0-9]+$/ || $i + 0 < 10) bad = 1
        }
    }
    END { exit (!found || bad) }
    ' "$table"
}

# The job's default shell, as the workflow writes it, run the way the runner
# runs it: the script file replaces {0}, the step's id is GITHUB_ACTION. The
# step runs under the wrapper with its own label, a status passes through, and
# `set -e` and `pipefail` hold in it, with a script path that has a space.
group_windows_shell() {
    test_table
    awk '/^  [a-z][a-z-]*:$/ { injob = ($1 == "completion-windows:") }
         injob && /^    steps:/ { exit }
         injob && /^        shell: / { print substr($0, index($0, "shell: ") + 7) }' "$workflow" > "$dir/default-shell"
    test "$(wc -l < "$dir/default-shell")" -eq 1
    grep -q 'run-check.pl "\$GITHUB_ACTION"' "$dir/default-shell"
    default_shell=$(sed 's|[{]0[}]|"$1"|' "$dir/default-shell")
    mkdir "$dir/step dir"
    run_step() {
        # $1: the step's script, $2: the id the runner puts in GITHUB_ACTION
        printf '%s\n' "$1" > "$dir/step dir/script.sh"
        (cd "$repo" && GITHUB_ACTION=$2 sh -c "$default_shell" sh "$dir/step dir/script.sh")
    }
    run_step 'echo ran-$((1 + 1))' within > "$dir/shell-ok.log" 2>&1
    grep -q '^== START within (limit' "$dir/shell-ok.log"
    grep -q '^ran-2$' "$dir/shell-ok.log"
    grep -q '^== END within: [0-9.]* s, exit 0 ==' "$dir/shell-ok.log"
    status=0
    run_step 'exit 17' windows-sched-deque > "$dir/shell-status.log" 2>&1 || status=$?
    test "$status" -eq 17
    grep -q '^== START windows-sched-deque ' "$dir/shell-status.log"
    status=0
    run_step 'false; echo after-$((1 + 1))-errexit' within > "$dir/shell-errexit.log" 2>&1 || status=$?
    test "$status" -ne 0
    if grep -q after-2-errexit "$dir/shell-errexit.log"; then exit 1; fi
    status=0
    run_step 'false | cat; echo after-$((1 + 1))-pipefail' within > "$dir/shell-pipefail.log" 2>&1 || status=$?
    test "$status" -ne 0
    if grep -q after-2-pipefail "$dir/shell-pipefail.log"; then exit 1; fi
    test ! -e "$dir/lock"
}

# The job's last step, as the workflow writes it: a record that no stage
# created, because no step ran under the wrapper, fails it, and says why; an
# empty record passes; an enforcing entry fails with the verdict.
group_windows_verdict() {
    awk '
    function emit() { if (collect && verdict) printf "%s", text; collect = 0; verdict = 0; text = "" }
    /^  [a-z][a-z-]*:$/ { emit(); injob = ($1 == "completion-windows:"); next }
    !injob { next }
    /^      - / { emit() }
    /^        run: [|]$/ { collect = 1; next }
    collect && /^          / { text = text substr($0, 11) "\n"; if ($0 ~ /--budget-verdict/) verdict = 1; next }
    END { emit() }
    ' "$workflow" > "$dir/verdict-step.sh"
    test -s "$dir/verdict-step.sh"
    run_verdict() {
        (cd "$repo" && env "$@" bash --noprofile --norc -eo pipefail "$dir/verdict-step.sh")
    }
    status=0
    run_verdict WHITEFOOT_X=1 > "$dir/verdict-unset.log" 2>&1 || status=$?
    test "$status" -eq 1
    grep -q 'no stage ran under run-check.pl' "$dir/verdict-unset.log"
    status=0
    run_verdict WHITEFOOT_TIME_BUDGET_RECORD="$dir/no-such-record" > "$dir/verdict-gone.log" 2>&1 || status=$?
    test "$status" -eq 1
    grep -q 'no stage ran under run-check.pl' "$dir/verdict-gone.log"
    : > "$dir/verdict-empty-record"
    run_verdict WHITEFOOT_TIME_BUDGET_RECORD="$dir/verdict-empty-record" > "$dir/verdict-clean.log" 2>&1
    printf '  ENFORCED: windows-x took 99.0 s, over its 10 s windows budget\n' > "$dir/verdict-over-record"
    status=0
    run_verdict WHITEFOOT_TIME_BUDGET_RECORD="$dir/verdict-over-record" > "$dir/verdict-over.log" 2>&1 || status=$?
    test "$status" -eq 1
    grep -q 'TIME BUDGETS EXCEEDED' "$dir/verdict-over.log"
}

# Each group runs in a subshell of its own in the background, with its trace
# kept, so that a failure names the group and the commands it ran last.
running=
for group in timeout grace cancelled signal orphan status exclusion parallelism \
    record classes macos hosts report_only record_file cpu cygwin missing_command \
    windows_form windows_shell windows_verdict; do
    mkdir "$work/$group"
    (
        dir=$work/$group
        WHITEFOOT_CHECK_LOCK_DIR=$dir/lock
        export WHITEFOOT_CHECK_LOCK_DIR
        set -x
        "group_$group"
    ) > "$work/$group/trace" 2>&1 &
    running="$running $group:$!"
done
failed=
for entry in $running; do
    if ! wait "${entry#*:}"; then
        failed="$failed ${entry%%:*}"
    fi
done
if [ -n "$failed" ]; then
    for group in $failed; do
        echo "check runner: the cases of group $group failed; its last commands:" >&2
        tail -n 12 "$work/$group/trace" >&2
    done
    exit 1
fi
echo 'check runner: status, nesting, exclusion, limits, timeout, cancellation, orphan cleanup and time budgets pass'
