#!/bin/sh
# Measurements of firn (apps/firn) against Redis and its competitors, driven by
# redis-benchmark: Experiments 7 and 8 of the io-model investigation (SHARED.md
# and TIME-AND-FILES.md), run then on the subset firn grew from, and the
# criteria of the firn investigation (research/investigations/firn/DESIGN.md).
# It builds firn with the worktree's compiler, checks every line for
# correctness, then measures the lines interleaved and prints one CSV line per
# run.
#
#   sh redis-bench.sh             the correctness pass and Experiments 7 and 8
#   sh redis-bench.sh verify      only the correctness pass
#   sh redis-bench.sh suite       the correctness pass and the firn criteria:
#                                 redis-benchmark's default suite on every line
#   sh redis-bench.sh compare     candidate, identical-image control, published
#                                 PR208 and pre208 on COMPARE_CPUS (default 1 2),
#                                 depths 1/16; exact revision labels required;
#                                 COMPARE_PILOT_ONLY=1 keeps preliminary sizing
#                                 and correctness observations without timings
#   sh redis-bench.sh scale       the suite's lines on each server CPU count in
#                                 SCALE (default 2 4 8 16), the client on the
#                                 host's other CPUs, for SCALE_TESTS at each
#                                 depth in SCALE_PIPELINES (default 16)
#   sh redis-bench.sh quick       firn (or firn-base with QUICK_LINE=firn-base)
#                                 on QUICK_CPUS server CPUs (default 4) against
#                                 the fastest of Garnet and Dragonfly, in under
#                                 two minutes: QUICK_TESTS (default all ten)
#                                 for QUICK_RUNS runs of QUICK_SECONDS each,
#                                 printing a table and marking each test below
#                                 QUICK_TARGET times the best other server;
#                                 QUICK_CLIENTS client processes (default one
#                                 per client CPU up to 16), each count with
#                                 its own kept reference
#
# firn is built with the options FIRN_LINK names, --full-lto when it is unset;
# the records before the quick mode built it with none.
#
# BASELINE_ROOT, when set, is a worktree of the revision before expiry with its
# compiler built; its subset is measured as the baseline lines of Experiment 8.
# DRAGONFLY and GARNET name those servers' executables; the suite skips a line
# whose executable is absent and says so. FIRN_BASELINE, when set, names
# another firn executable, such as one built from an earlier revision, which
# the suite measures as the firn-base lines beside firn.
#
# The servers run pinned to SERVER_CPUS and the client to CLIENT_CPUS, so the
# two never share a core; nothing else should run on the host meanwhile. The
# suite runs each line both on two server CPUs (0 and 1, the client on 2 and 3)
# and on one (0, the client on 1 to 3).
set -e

ROOT=${ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
OUT=${OUT:-/tmp/redis-bench}
WHITEFOOTC=${WHITEFOOTC:-$ROOT/compiler/target/gate/whitefootc}
BASELINE_ROOT=${BASELINE_ROOT:-}
DRAGONFLY=${DRAGONFLY:-dragonfly}
GARNET=${GARNET:-garnet-server}
FIRN_BASELINE=${FIRN_BASELINE:-}
SERVER_CPUS=${SERVER_CPUS:-0,1}
CLIENT_CPUS=${CLIENT_CPUS:-2,3}
CLIENT_THREADS=${CLIENT_THREADS:-2}
REQUESTS=${REQUESTS:-1000000}
ROUNDS=${ROUNDS:-2}
PASSES=${PASSES:-3}
SECONDS_PER_RUN=${SECONDS_PER_RUN:-12}
# A server that closes an idle client first holds that port in TIME_WAIT for
# a minute, and a server that does not set SO_REUSEADDR cannot listen on it
# meanwhile, so each run starts its ports from its own process number rather
# than from one fixed port a run a minute earlier may still hold.
PORT=${PORT:-$((10000 + $$ % 400 * 50))}
MODE=${1:-bench}
HOST_OS=$(uname -s)
# Darwin has no CPU-affinity equivalent here. Compare mode explicitly runs
# unpinned; its driver count is not described as a reserved CPU count.
pinned() {
    if [ "$MODE" = compare ] && [ "$HOST_OS" = Darwin ]; then
        shift 2
        "$@"
    else
        taskset "$@"
    fi
}
server_pinned() {
    if [ "$MODE" = compare ] && [ "$HOST_OS" = Darwin ]; then
        shift 2
        exec "$@"
    else
        exec taskset "$@"
    fi
}

mkdir -p "$OUT"
# firn is linked as a server would be, its module and the runtime's units
# optimized together; FIRN_LINK names other link options, or none.
if [ "$MODE" != compare ]; then
    "$WHITEFOOTC" ${FIRN_LINK---full-lto} --graph "$ROOT/apps/firn/modules.wfg" --entry firn -o "$OUT/firn"
fi
baselines=
if [ -n "$BASELINE_ROOT" ]; then
    "$BASELINE_ROOT/compiler/target/gate/whitefootc" -o "$OUT/redis_baseline" \
        "$BASELINE_ROOT/tests/programs/redis_subset.wf"
    baselines="baseline-2 baseline-1"
fi

server=
sampler=
# IDLE, when set, is the idle limit in seconds a start gives the server; KEEP,
# when set, keeps the append-only file a previous start left.
IDLE=
KEEP=
# Whether a line's server can run on this host.
available() {
    case $1 in
        dragonfly-*) command -v "$DRAGONFLY" >/dev/null 2>&1 ;;
        garnet-*) command -v "$GARNET" >/dev/null 2>&1 ;;
        firn-base-*) test -n "$FIRN_BASELINE" && test -x "$FIRN_BASELINE" ;;
        valkey*) command -v valkey-server >/dev/null 2>&1 ;;
        *) true ;;
    esac
}
# The number of CPUs in a taskset list such as 0,1.
cpu_count() {
    echo "$1" | tr ',' '\n' | wc -l
}
# Each start takes a fresh port: a stopped server's accepted connections wait
# out TIME_WAIT on its port, which a server without SO_REUSEADDR cannot bind.
start() {
    PORT=$((PORT + 1))
    cpus=${COMPARE_DRIVERS:-$(cpu_count "$SERVER_CPUS")}
    if [ -z "$KEEP" ]; then
        rm -rf "$OUT/appendonlydir" "$OUT/firn.aof"
    fi
    case $1 in
        candidate|identical|published|pre208)
            case $1 in
                candidate|identical) image=$FIRN_CANDIDATE ;;
                published) image=$FIRN_PUBLISHED ;;
                pre208) image=$FIRN_PRE208 ;;
            esac
            WF_DRIVERS=$cpus server_pinned -c "$SERVER_CPUS" "$image" "$PORT" 0 - 0 \
                >"$OUT/server-$1.log" 2>&1 &
            ;;
        reference)
            taskset -c "$SERVER_CPUS" redis-server --port "$PORT" --save "" \
                --appendonly no --timeout "${IDLE:-0}" --daemonize no \
                >"$OUT/server.log" 2>&1 &
            ;;
        reference-aof)
            taskset -c "$SERVER_CPUS" redis-server --port "$PORT" --save "" \
                --appendonly yes --appendfsync everysec --dir "$OUT" \
                --timeout "${IDLE:-0}" --daemonize no \
                >"$OUT/server.log" 2>&1 &
            ;;
        valkey)
            taskset -c "$SERVER_CPUS" valkey-server --port "$PORT" --save "" \
                --appendonly no --daemonize no >"$OUT/server.log" 2>&1 &
            ;;
        valkey-io)
            taskset -c "$SERVER_CPUS" valkey-server --port "$PORT" --save "" \
                --appendonly no --io-threads "$cpus" --io-threads-do-reads yes \
                --daemonize no >"$OUT/server.log" 2>&1 &
            ;;
        dragonfly-*)
            taskset -c "$SERVER_CPUS" "$DRAGONFLY" --port="$PORT" \
                --proactor_threads="${1#dragonfly-}" --dbfilename= \
                --logtostderr >"$OUT/server.log" 2>&1 &
            ;;
        garnet-*)
            taskset -c "$SERVER_CPUS" "$GARNET" --port "$PORT" \
                --bind 127.0.0.1 >"$OUT/server.log" 2>&1 &
            ;;
        firn-base-*)
            WF_DRIVERS=${1#firn-base-} taskset -c "$SERVER_CPUS" \
                "$FIRN_BASELINE" "$PORT" 0 - "${IDLE:-0}" \
                >"$OUT/server.log" 2>&1 &
            ;;
        firn-aof-*)
            (cd "$OUT" && WF_DRIVERS=${1##*-} exec taskset -c "$SERVER_CPUS" \
                ./firn "$PORT" 0 firn.aof "${IDLE:-0}") \
                >"$OUT/server.log" 2>&1 &
            ;;
        firn-*)
            WF_DRIVERS=${1#firn-} taskset -c "$SERVER_CPUS" \
                "$OUT/firn" "$PORT" 0 - "${IDLE:-0}" \
                >"$OUT/server.log" 2>&1 &
            ;;
        baseline-*)
            WF_DRIVERS=${1#baseline-} taskset -c "$SERVER_CPUS" \
                "$OUT/redis_baseline" "$PORT" 0 >"$OUT/server.log" 2>&1 &
            ;;
    esac
    server=$!
    tries=0
    until redis-cli -p "$PORT" PING 2>/dev/null | grep -q PONG; do
        tries=$((tries + 1))
        if [ "$tries" -gt 400 ]; then
            echo "$1 never answered on $PORT" >&2
            exit 1
        fi
        sleep 0.05
    done
}

stop() {
    kill "$server"
    wait "$server" 2>/dev/null || true
    server=
}

fail() {
    echo "$1 failed the correctness pass: $2" >&2
    exit 1
}

# Correct: no error reply in a mixed run, and every one of 100,000 increments
# from 50 clients reaches the one counter.
verify() {
    start "$1"
    pinned -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" -t incr -n 100000 \
        -c 50 -q >"$OUT/verify-$1.txt" 2>&1
    counted=$(redis-cli -p "$PORT" GET counter:__rand_int__)
    replies=$(printf 'SET a 1\nGET a\nINCR a\nDEL a\nGET a\n' |
        redis-cli -p "$PORT" | tr '\n' ' ')
    stop
    echo "verify,$1,counter=$counted,replies=$replies"
    if [ "$counted" != 100000 ] || [ "$replies" != "OK 1 2 1  " ]; then
        fail "$1" "counter or replies"
    fi
}

# Experiment 8: a key set with PX 100 answers a positive PTTL and is absent
# 200 milliseconds later; TTL answers -1 without an expiry and -2 for a
# missing key; PERSIST removes an expiry once.
verify_expiry() {
    start "$1"
    replies=$(printf 'SET kept 1\nSET brief hello PX 100\nTTL kept\nTTL absent\nEXPIRE kept 100\nTTL kept\nPERSIST kept\nTTL kept\nPERSIST kept\n' |
        redis-cli -p "$PORT" | tr '\n' ' ')
    left=$(redis-cli -p "$PORT" PTTL brief)
    sleep 0.2
    after=$(printf 'GET brief\nPTTL brief\n' | redis-cli -p "$PORT" | tr '\n' ' ')
    stop
    echo "verify-expiry,$1,replies=$replies,left=$left,after=$after"
    if [ "$replies" != "OK OK -1 -2 1 100 1 -1 0 " ] ||
        [ "$left" -le 0 ] || [ "$left" -gt 100 ] || [ "$after" != " -2 " ]; then
        fail "$1" "expiry replies"
    fi
}

# Experiment 8: after a stop and a restart on the file, a key set and not
# expired holds its value and a key whose expiry passed meanwhile is absent;
# a key made persistent before its expiry holds its value, and one incremented
# before its expiry passed is absent, as a replay that expires nothing while it
# loads leaves them.
# firn stops by being killed, so the check waits 100 milliseconds for its
# writer, which appends every 10, before stopping it; Redis flushes its file
# when it is stopped.
verify_restart() {
    start "$1"
    printf 'SET k1 v\nSET k2 v PX 300\nSET k3 v EX 100\nINCR n\nINCR n\nDEL k1\nSET k4 v\nSET k5 v PX 300\nPERSIST k5\nSET n2 5 PX 300\nINCR n2\n' |
        redis-cli -p "$PORT" >/dev/null
    sleep 0.1
    stop
    sleep 0.5
    KEEP=1
    start "$1"
    KEEP=
    got=$(printf 'GET k1\nGET k2\nGET k3\nGET n\nGET k4\nGET k5\nGET n2\n' |
        redis-cli -p "$PORT" | tr '\n' ' ')
    left=$(redis-cli -p "$PORT" TTL k3)
    stop
    echo "verify-restart,$1,got=$got,left=$left"
    if [ "$got" != "  v 2 v v  " ] || [ "$left" -lt 98 ]; then
        fail "$1" "replayed keys"
    fi
}

# Experiment 8: a connection silent past a one-second limit is closed within
# two seconds.
verify_idle() {
    IDLE=1
    start "$1"
    IDLE=
    silent=$(python3 - "$PORT" <<'EOF'
import socket, sys, time
connection = socket.create_connection(("127.0.0.1", int(sys.argv[1])))
connection.sendall(b"*1\r\n$4\r\nPING\r\n")
connection.recv(64)
started = time.monotonic()
connection.settimeout(10)
connection.recv(64)
print("%.2f" % (time.monotonic() - started))
EOF
)
    stop
    echo "verify-idle,$1,closed after $silent s"
    if ! awk "BEGIN { exit !($silent < 2) }"; then
        fail "$1" "idle limit"
    fi
}

# Experiment 8: after 100,000 keys set with PX 1000 and no further reads,
# DBSIZE falls below 1 percent of them within ten seconds of the last set.
verify_active() {
    start "$1"
    removed=$(python3 - "$PORT" <<'EOF'
import socket, sys, time
KEYS = 100000
connection = socket.create_connection(("127.0.0.1", int(sys.argv[1])))
batch = bytearray()
for index in range(KEYS):
    key = b"active:%d" % index
    batch += b"*5\r\n$3\r\nSET\r\n$%d\r\n%s\r\n$1\r\nv\r\n$2\r\nPX\r\n$4\r\n1000\r\n" % (len(key), key)
connection.sendall(batch)
expected = b"+OK\r\n" * KEYS
held = bytearray()
while len(held) < len(expected):
    held += connection.recv(1 << 16)
assert held == expected, held[:64]
started = time.monotonic()
reader = connection.makefile("rb")
while True:
    connection.sendall(b"*1\r\n$6\r\nDBSIZE\r\n")
    size = int(reader.readline()[1:])
    elapsed = time.monotonic() - started
    if size < KEYS // 100 or elapsed > 10:
        print("%d keys left after %.2f s" % (size, elapsed))
        break
    time.sleep(0.1)
EOF
)
    stop
    echo "verify-active,$1,$removed"
    case $removed in
        *" after "*) left=${removed%% keys*} ;;
        *) left=100000 ;;
    esac
    if [ "$left" -ge 1000 ]; then
        fail "$1" "active expiry"
    fi
}

# The firn criteria: every test of the default suite completes on the line
# with no error reply.
verify_suite() {
    start "$1"
    pinned -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" -c 50 -n 20000 \
        -r 100000 --threads "$CLIENT_THREADS" --csv >"$OUT/suite-$1.csv" \
        2>"$OUT/suite-$1.err"
    stop
    tests=$(grep -c -v '^"test"' "$OUT/suite-$1.csv")
    echo "verify-suite,$1,$tests tests"
    # redis-benchmark reads the server's CONFIG only to report it; Dragonfly
    # does not answer it as Redis does, which the client warns about and
    # which changes nothing it measures. Any other message is a failure.
    grep -v '^WARNING: Could not fetch server CONFIG$' "$OUT/suite-$1.err" \
        >"$OUT/suite-$1.problems" || true
    if [ "$tests" != 20 ] || [ -s "$OUT/suite-$1.problems" ]; then
        fail "$1" "the default suite"
    fi
}

measure() {
    start "$1"
    for pipeline in 1 16; do
        taskset -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" \
            --threads "$CLIENT_THREADS" -c 50 -n "$REQUESTS" -r 100000 -d 16 \
            -t set,get -P "$pipeline" --csv 2>/dev/null | grep -v '^"test"' |
            sed "s/^/$1,round $2,pipeline $pipeline,/"
    done
    stop
}

SUITE_TESTS=${SUITE_TESTS:-"ping_inline ping_mbulk set get incr lpush rpush lpop rpop sadd hset spop zadd zpopmin lrange_100 lrange_300 lrange_500 lrange_600 mset"}
# The depths the suite and its pilot run each test at.
PIPELINES=${PIPELINES:-"1 16"}

# One test's rate on the running server at one depth after a given number of
# requests, the list refill of an LRANGE test left out.
pilot_rate() {
    taskset -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" \
        --threads "$CLIENT_THREADS" -c 50 -n "$3" -r 100000 -t "$1" -P "$2" \
        --csv 2>/dev/null | grep -v '^"test"' | grep -v '^"LPUSH (needed' |
        head -1 | cut -d, -f2 | tr -d '"'
}

# The requests a suite run of one test at one depth sends: SECONDS_PER_RUN
# seconds at the faster of Redis's and firn's rate, so that every line runs at
# least that long at the rate of the faster of them and redis-benchmark's
# quarter-second clock reads its rate to within about 2 percent. Each rate is
# read in two steps, 100,000 requests and then about two seconds' worth, since
# the clock cannot read a shorter run. The pilot's rates are printed as pilot
# lines.
pilot() {
    rm -f "$OUT/pilot.csv"
    for line in reference "firn-$(cpu_count "$SERVER_CPUS")"; do
        start "$line"
        for pipeline in $PIPELINES; do
            for test in $SUITE_TESTS; do
                first=$(pilot_rate "$test" "$pipeline" 100000)
                requests=$(awk -v rate="$first" 'BEGIN {
                    n = int(rate * 2); if (n < 100000) n = 100000; print n }')
                rate=$(pilot_rate "$test" "$pipeline" "$requests")
                echo "$line,$pipeline,$test,$rate" >>"$OUT/pilot.csv"
            done
        done
        stop
    done
    sed 's/^/pilot,/' "$OUT/pilot.csv"
}

requests_for() {
    awk -F, -v test="$1" -v pipeline="$2" -v seconds="$SECONDS_PER_RUN" '
        $2 == pipeline && $3 == test { if ($4 + 0 > rate) rate = $4 + 0 }
        END { printf "%d\n", rate * seconds + 1 }' "$OUT/pilot.csv"
}

suite_run() {
    start "$1"
    for pipeline in $PIPELINES; do
        for test in $SUITE_TESTS; do
            requests=$(requests_for "$test" "$pipeline")
            taskset -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" \
                --threads "$CLIENT_THREADS" -c 50 -n "$requests" -r 100000 \
                -t "$test" -P "$pipeline" --csv 2>/dev/null |
                grep -v '^"test"' | grep -v '^"LPUSH (needed' |
                sed "s/^/$1,$2,$3,pipeline $pipeline,/"
        done
    done
    stop
}

# One run of the quick comparison: <requests> of <test> from one
# single-threaded redis-benchmark per client CPU with 3 connections each,
# printing the rate as the requests over the wall time read outside the
# processes. A threaded redis-benchmark ends only on a tick of about 250 ms,
# so its short runs resolve nothing, and one process is the limit on 8 or
# more server CPUs. LRANGE_100 is sent as its command after 100,000 pushes
# outside the timed span, since the RPOP runs before it may empty the list.
quick_client() {
    case $2 in
        lrange_100)
            redis-benchmark -p "$PORT" -t lpush -n 100000 -P 16 -q >/dev/null 2>&1
            what="LRANGE mylist 0 99"
            ;;
        *) what="-t $2" ;;
    esac
    begin=$(date +%s%N)
    pids=
    for cpu in $(echo "$CLIENT_CPUS" | tr ',' ' '); do
        taskset -c "$cpu" redis-benchmark -p "$PORT" -c 3 \
            -n $(($1 / CLIENT_THREADS)) -r 100000 -P 16 -q $what >/dev/null 2>&1 &
        pids="$pids $!"
    done
    wait $pids
    end=$(date +%s%N)
    awk -v requests=$(($1 / CLIENT_THREADS * CLIENT_THREADS)) -v ns=$((end - begin)) \
        'BEGIN { printf "%d\n", requests / (ns / 1e9) }'
}

# Measures <line> on <tests>: a short run sizes each test to QUICK_SECONDS,
# then QUICK_RUNS runs of every test in turn; each run goes to
# quick-runs-<line>.csv and each test's median to <file> as line,test,rate.
quick_measure() {
    start "$1"
    runs="$OUT/quick-runs-$1.csv"
    : >"$runs"
    : >"$OUT/quick-requests.txt"
    for test in $3; do
        first=$(quick_client 1600000 "$test")
        echo "$test $((first * ${QUICK_SECONDS:-3}))" >>"$OUT/quick-requests.txt"
    done
    run=1
    while [ "$run" -le "${QUICK_RUNS:-3}" ]; do
        for test in $3; do
            requests=$(awk -v t="$test" '$1 == t { print $2 }' "$OUT/quick-requests.txt")
            echo "$1,$test,$run,$(quick_client "$requests" "$test")" >>"$runs"
        done
        run=$((run + 1))
    done
    stop
    for test in $3; do
        awk -F, -v t="$test" '$2 == t { print $4 }' "$runs" | sort -n |
            awk -v line="$1" -v t="$test" '
                { rate[NR] = $1 }
                END { print line "," t "," rate[int((NR + 1) / 2)] }' >>"$2"
    done
}

# The quick comparison, for the loop of changing firn and measuring again:
# firn on QUICK_CPUS server CPUs against the fastest of Garnet and Dragonfly,
# which led every test of the scaling run. Those two are measured once per
# CPU count and client count and kept in quick-ref-<n>-<clients>.csv until
# QUICK_REFRESH is set; nothing
# is verified. Its rates are its own: its client is not the suite's.
# An explicitly invoked recovery experiment, separate from the historical
# suite. All four lines get the same request count and CPU placement per cell.
# The identical line is the exact candidate executable, not another build.
if [ "$MODE" = compare ]; then
    trap 'if [ -n "$sampler" ]; then kill "$sampler" 2>/dev/null || true; wait "$sampler" 2>/dev/null || true; fi; if [ -n "$server" ]; then kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true; fi' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    : "${FIRN_CANDIDATE:?candidate executable required}"
    : "${FIRN_PUBLISHED:?published PR208 executable required}"
    : "${FIRN_PRE208:?pre208 executable required}"
    : "${CANDIDATE_REVISION:?complete candidate revision required}"
    : "${PUBLISHED_REVISION:?complete published revision required}"
    : "${PRE208_REVISION:?complete pre208 revision required}"
    for image in "$FIRN_CANDIDATE" "$FIRN_PUBLISHED" "$FIRN_PRE208"; do
        test -x "$image" || { echo "not executable: $image" >&2; exit 1; }
    done
    for revision in "$CANDIDATE_REVISION" "$PUBLISHED_REVISION" "$PRE208_REVISION"; do
        case $revision in *[!0-9a-f]*) echo "not a commit SHA: $revision" >&2; exit 1 ;; esac
        test "${#revision}" -eq 40 || { echo "complete SHA required" >&2; exit 1; }
    done
    if [ "$HOST_OS" = Darwin ]; then
        usable=unpinned
        total=$(sysctl -n hw.logicalcpu)
    else
        # sched_getaffinity lists usable IDs, including container cpusets.
        usable=$(python3 - <<'CPU_IDS'
import os
print(" ".join(map(str, sorted(os.sched_getaffinity(0)))))
CPU_IDS
)
        total=$(echo "$usable" | wc -w)
    fi
    {
        uname -a
        echo "usable CPUs: $usable"
        if [ "$HOST_OS" = Darwin ]; then
            sw_vers
            sysctl hw.model hw.logicalcpu hw.physicalcpu hw.memsize machdep.cpu.brand_string
            echo 'Placement: unpinned; server and client may share cores.'
            echo 'server_cpus column denotes requested driver count, not reserved CPUs.'
            echo 'Memory: ps RSS samples, not a continuous peak or allocation count.'
        else
            cat /proc/cpuinfo
            cat /proc/meminfo
            cat /proc/loadavg
            taskset --version
            echo 'Memory: /proc VmRSS and VmHWM, not allocation counts.'
        fi
        redis-cli --version
        redis-benchmark --version
        echo "redis_client_revision,${REDIS_CLIENT_REVISION:-system package; version recorded above}"
        echo "candidate,$CANDIDATE_REVISION,$FIRN_CANDIDATE"
        echo "identical,$CANDIDATE_REVISION,$FIRN_CANDIDATE"
        echo "published,$PUBLISHED_REVISION,$FIRN_PUBLISHED"
        echo "pre208,$PRE208_REVISION,$FIRN_PRE208"
        if [ "$HOST_OS" = Darwin ]; then
            shasum -a 256 "$(command -v redis-cli)" "$(command -v redis-benchmark)"
            shasum -a 256 "$FIRN_CANDIDATE" "$FIRN_PUBLISHED" "$FIRN_PRE208"
        else
            sha256sum "$(command -v redis-cli)" "$(command -v redis-benchmark)"
            sha256sum "$FIRN_CANDIDATE" "$FIRN_PUBLISHED" "$FIRN_PRE208"
        fi
        echo "passes=${COMPARE_PASSES:-6},warmup_requests=100000,seconds=${SECONDS_PER_RUN},clients=50,random_keys=100000,value_bytes=3,pipelines=1 16,tests=mset set get"
    } >"$OUT/compare-host.txt"
    echo 'line,revision,server_cpus,pass,pipeline,test,rps,avg_ms,min_ms,p50_ms,p95_ms,p99_ms,max_ms' >"$OUT/compare.csv"
    echo 'line,revision,server_cpus,pass,pipeline,test,rss_kB,peak_rss_kB,memory_method' >"$OUT/compare-memory.csv"
    # Redis 7.0.15's built-in GET randomizes key:__rand_int__ to twelve
    # decimal digits (redis-benchmark.c randomizeClientKey/default GET).
    # Populate its complete domain before every GET pilot and measured cell.
    python3 - "$OUT/compare-get-data.resp" <<'GET_DATA'
import sys
with open(sys.argv[1], "wb") as out:
    for index in range(100000):
        parts = (b"SET", f"key:{index:012d}".encode(), b"x" * 3)
        out.write(b"*3\r\n")
        for part in parts:
            out.write(f"${len(part)}\r\n".encode() + part + b"\r\n")
GET_DATA
    echo 'GET dataset: all 100000 key:000000000000..key:000000099999, 3 x bytes, hit-only' >>"$OUT/compare-host.txt"
    compare_populate() {
        redis-cli -p "$PORT" --pipe <"$OUT/compare-get-data.resp" >"$OUT/populate-$line-$n-$pass-$depth.txt"
        grep -q '^errors: 0, replies: 100000$' "$OUT/populate-$line-$n-$pass-$depth.txt" || exit 1
        test "$(redis-cli -p "$PORT" DBSIZE)" = 100000 || exit 1
        test "$(redis-cli -p "$PORT" GET key:000000000000)" = xxx || exit 1
        test "$(redis-cli -p "$PORT" GET key:000000099999)" = xxx || exit 1
    }
    # Every client result is read only after redis-benchmark's exit succeeds.
    # Empty/malformed CSV or unexpected diagnostics fail this experiment.
    compare_client() {
        file="$OUT/client-$1-$2-$3-$4-$5.csv"
        pinned -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" \
            --threads "$CLIENT_THREADS" -c 50 -n "$6" -r 100000 -d 3 \
            -t "$5" -P "$4" --csv >"$file" 2>"$file.err"
        python3 - "$file" "$5" <<'CHECK_CSV'
import csv, math, pathlib, sys
path, test = pathlib.Path(sys.argv[1]), sys.argv[2].upper()
if test == "MSET":
    test = "MSET (10 keys)"
errors = [line for line in pathlib.Path(str(path) + ".err").read_text().splitlines()
          if line != "WARNING: Could not fetch server CONFIG"]
rows = [row for row in csv.reader(path.open()) if row and row[0] != "test"]
if errors or len(rows) != 1 or len(rows[0]) != 8 or rows[0][0] != test:
    raise SystemExit(f"unexpected client output: {path}, {rows}, {errors}")
values = [float(value) for value in rows[0][1:]]
if not all(math.isfinite(value) and value >= 0 for value in values) or values[0] <= 0:
    raise SystemExit(f"invalid metrics: {path}")
CHECK_CSV
    }
    for n in ${COMPARE_CPUS:-1 2}; do
        case $n in ''|0*|*[!0-9]*) echo "invalid CPU count: $n" >&2; exit 1 ;; esac
        test "$n" -gt 0 || exit 1
        if [ "$n" -ge "$total" ]; then
            echo "skip,compare $n,only $total host CPUs; requested count leaves no client capacity"
            continue
        fi
        COMPARE_DRIVERS=$n
        if [ "$HOST_OS" = Darwin ]; then
            SERVER_CPUS=unpinned
            CLIENT_CPUS=unpinned
            CLIENT_THREADS=${COMPARE_CLIENT_THREADS:-2}
        else
            SERVER_CPUS=$(echo "$usable" | awk -v n="$n" '{for(i=1;i<=n;i++) printf "%s%s",(i>1?",":""),$i}')
            CLIENT_CPUS=$(echo "$usable" | awk -v n="$n" '{for(i=n+1;i<=NF && i<=n+16;i++) printf "%s%s",(i>n+1?",":""),$i}')
            CLIENT_THREADS=$(cpu_count "$CLIENT_CPUS")
        fi
        echo "placement,$n,$SERVER_CPUS,$CLIENT_CPUS,$CLIENT_THREADS" >>"$OUT/compare-host.txt"
        # Choose identical request counts using the fastest pilot of all images.
        echo 'line,revision,pipeline,test,rps' >"$OUT/compare-pilot-$n.csv"
        for line in candidate identical published pre208; do
            case $line in
                candidate|identical) revision=$CANDIDATE_REVISION ;;
                published) revision=$PUBLISHED_REVISION ;;
                pre208) revision=$PRE208_REVISION ;;
            esac
            verify "$line"
            verify_suite "$line"
            pass=pilot
            for depth in 1 16; do
                for test in mset set get; do
                    start "$line"
                    if [ "$test" = get ]; then compare_populate; fi
                    compare_client "$line" "$n" pilot "$depth" "$test" 100000
                    rate=$(awk -F, '$1!="\"test\"" {gsub(/"/,"",$2); print $2}' "$file")
                    # A second sizing run lasts about two seconds, avoiding
                    # gross comparisons from the client's 250-ms end tick.
                    sizing=$(awk -v rate="$rate" 'BEGIN {n=int(rate*2); if(n<100000)n=100000; print n}')
                    compare_client "$line" "$n" "pilot-sized" "$depth" "$test" "$sizing"
                    rate=$(awk -F, '$1!="\"test\"" {gsub(/"/,"",$2); print $2}' "$file")
                    echo "$line,$revision,$depth,$test,$rate" >>"$OUT/compare-pilot-$n.csv"
                    stop
                done
            done
        done
        if [ "${COMPARE_PILOT_ONLY:-0}" = 1 ]; then
            echo "preliminary sizing only,$n drivers; recovery has not been measured"
            cat "$OUT/compare-pilot-$n.csv"
            continue
        fi
        pass=1
        while [ "$pass" -le "${COMPARE_PASSES:-6}" ]; do
            # Forward/reverse pairs put each image equally in its two
            # opposite positions across the predeclared six rounds.
            case $((pass % 2)) in
                1) lines="candidate identical published pre208" ;;
                0) lines="pre208 published identical candidate" ;;
            esac
            for line in $lines; do
                case $line in
                    candidate|identical) revision=$CANDIDATE_REVISION ;;
                    published) revision=$PUBLISHED_REVISION ;;
                    pre208) revision=$PRE208_REVISION ;;
                esac
                for depth in 1 16; do
                    for test in mset set get; do
                        # Fresh server per cell avoids other tests' state and
                        # makes each RSS high-water observation attributable.
                        start "$line"
                        if [ "$test" = get ]; then compare_populate; fi
                        compare_client "$line" "$n" "warmup-$pass" "$depth" "$test" 100000
                        requests=$(awk -F, -v p="$depth" -v t="$test" -v seconds="$SECONDS_PER_RUN" '$3==p && $4==t {if($5+0>rate)rate=$5+0} END {printf "%d\n", rate*seconds+1}' "$OUT/compare-pilot-$n.csv")
                        sampler=
                        if [ "$HOST_OS" = Darwin ]; then
                            samples="$OUT/rss-$line-$n-$pass-$depth-$test.txt"
                            (
                                while kill -0 "$server" 2>/dev/null; do
                                    ps -o rss= -p "$server" || exit 1
                                    sleep 0.1
                                done
                            ) >"$samples" &
                            sampler=$!
                        fi
                        compare_client "$line" "$n" "$pass" "$depth" "$test" "$requests"
                        if [ -n "$sampler" ]; then
                            kill "$sampler"
                            wait "$sampler" 2>/dev/null || true
                            sampler=
                        fi
                        python3 - "$file" "$line" "$revision" "$n" "$pass" "$depth" >>"$OUT/compare.csv" <<'WRITE_CSV'
import csv, sys
rows = [row for row in csv.reader(open(sys.argv[1])) if row and row[0] != "test"]
csv.writer(sys.stdout).writerow(sys.argv[2:] + rows[0])
WRITE_CSV
                        if [ "$HOST_OS" = Darwin ]; then
                            rss=$(ps -o rss= -p "$server" | tr -d ' ')
                            peak=$(awk 'NF {if($1>peak)peak=$1} END {print peak+0}' "$samples")
                            echo "$line,$revision,$n,$pass,$depth,$test,$rss,$peak,ps_sampled" >>"$OUT/compare-memory.csv"
                        else
                            awk -v prefix="$line,$revision,$n,$pass,$depth,$test" '/^VmRSS:/ {rss=$2} /^VmHWM:/ {hwm=$2} END {if(rss=="" || hwm=="") exit 1; print prefix "," rss "," hwm ",proc_status"}' "/proc/$server/status" >>"$OUT/compare-memory.csv"
                        fi
                        stop
                    done
                done
            done
            pass=$((pass + 1))
        done
    done
    if [ "${COMPARE_PILOT_ONLY:-0}" = 1 ]; then
        echo 'Preliminary correctness and sizing only; no completed recovery comparison.'
        exit 0
    fi
    python3 - "$OUT/compare.csv" "$OUT/compare-memory.csv" "${COMPARE_PASSES:-6}" <<'CHECK_MATRIX'
import csv, sys
rows = list(csv.DictReader(open(sys.argv[1])))
memory = list(csv.DictReader(open(sys.argv[2])))
if not rows or len(memory) != len(rows):
    raise SystemExit("missing measured cells or memory observations")
counts = {row["server_cpus"] for row in rows}
expected = {(line, n, str(p), depth, test) for line in ("candidate", "identical", "published", "pre208")
            for n in counts for p in range(1, int(sys.argv[3]) + 1)
            for depth in ("1", "16") for test in ("MSET (10 keys)", "SET", "GET")}
actual = [(r["line"], r["server_cpus"], r["pass"], r["pipeline"], r["test"]) for r in rows]
if len(actual) != len(expected) or set(actual) != expected:
    raise SystemExit("duplicate or missing sample cells")
def memory_test(test):
    return "MSET (10 keys)" if test == "mset" else test.upper()
observations = [(r["line"], r["server_cpus"], r["pass"], r["pipeline"], memory_test(r["test"])) for r in memory]
if len(set(observations)) != len(expected) or set(observations) != expected:
    raise SystemExit("duplicate or mismatched memory cells")
revisions = {r["line"]: r["revision"] for r in rows}
for row in memory:
    if (row["revision"] != revisions[row["line"]] or int(row["rss_kB"]) <= 0 or
            int(row["peak_rss_kB"]) <= 0 or row["memory_method"] not in ("proc_status", "ps_sampled")):
        raise SystemExit("invalid memory observation")
CHECK_MATRIX
    cat "$OUT/compare.csv"
    exit 0
fi

if [ "$MODE" = quick ]; then
    n=${QUICK_CPUS:-4}
    total=$(nproc)
    all="set get incr lpush rpop sadd hset zadd lrange_100 mset"
    CLIENT_THREADS=${QUICK_CLIENTS:-$((total - n < 16 ? total - n : 16))}
    SERVER_CPUS=$(seq -s, 0 $((n - 1)))
    CLIENT_CPUS=$(seq -s, "$n" $((n + CLIENT_THREADS - 1)))
    reference="$OUT/quick-ref-$n-$CLIENT_THREADS.csv"
    if [ -n "$QUICK_REFRESH" ] || [ ! -s "$reference" ]; then
        : >"$reference.new"
        for line in "garnet-$n" "dragonfly-$n"; do
            if available "$line"; then
                quick_measure "$line" "$reference.new" "$all"
            else
                echo "skip,$line,no executable"
            fi
        done
        mv "$reference.new" "$reference"
    fi
    line=${QUICK_LINE:-firn}-$n
    : >"$OUT/quick-$n.csv"
    quick_measure "$line" "$OUT/quick-$n.csv" "${QUICK_TESTS:-$all}"
    # The ratio every test is asked to reach, the owner's aim of 2026-10-02.
    awk -F, -v target="${QUICK_TARGET:-1.4}" -v n="$n" -v line="$line" '
        FNR == NR { if ($3 + 0 > best[$2]) { best[$2] = $3 + 0; by[$2] = $1 } next }
        FNR == 1 {
            printf "%-11s %9s %9s %-12s %6s\n", "n=" n, line, "best", "", "ratio"
        }
        {
            ratio = $3 / best[$2]
            if (ratio < target) below++
            printf "%-11s %9.0f %9.0f %-12s %6.2f %s\n", $2, $3 / 1000,
                best[$2] / 1000, by[$2], ratio, (ratio < target ? "BELOW" : "")
        }
        END { printf "%d of %d below %s\n", below, FNR, target }' \
        "$reference" "$OUT/quick-$n.csv"
    exit 0
fi

# The scaling run: on each server CPU count n in SCALE, the servers on CPUs 0
# to n - 1 and the client on the rest, with one client thread per client CPU
# up to 16; every line is checked on n CPUs, a pilot sizes the runs for n,
# and then PASSES passes measure the lines interleaved.
if [ "$MODE" = scale ]; then
    total=$(nproc)
    SUITE_TESTS=${SCALE_TESTS:-"set get incr lpush rpop sadd hset zadd lrange_100 mset"}
    PIPELINES=${SCALE_PIPELINES:-16}
    for n in ${SCALE:-2 4 8 16}; do
        if [ "$n" -ge "$total" ]; then
            echo "skip,scale $n,the host has $total CPUs"
            continue
        fi
        SERVER_CPUS=$(seq -s, 0 $((n - 1)))
        CLIENT_CPUS=$(seq -s, "$n" $((total - 1)))
        CLIENT_THREADS=$((total - n < 16 ? total - n : 16))
        lines="reference valkey-io dragonfly-$n garnet-$n firn-$n firn-base-$n"
        for line in $lines; do
            if available "$line"; then
                verify_suite "$line"
            else
                echo "skip,$line,no executable"
            fi
        done
        pilot
        pass=1
        while [ "$pass" -le "$PASSES" ]; do
            for line in $lines; do
                if available "$line"; then
                    suite_run "$line" "pass $pass" "$n server CPUs"
                fi
            done
            pass=$((pass + 1))
        done
    done
    exit 0
fi

if [ "$MODE" = suite ]; then
    two="reference valkey valkey-io dragonfly-2 garnet-2 firn-2 firn-base-2"
    one="reference valkey dragonfly-1 garnet-1 firn-1 firn-base-1"
    for line in $two; do
        if available "$line"; then
            verify_suite "$line"
        else
            echo "skip,$line,no executable"
        fi
    done
    SERVER_CPUS=0,1
    CLIENT_CPUS=2,3
    CLIENT_THREADS=2
    pilot
    pass=1
    while [ "$pass" -le "$PASSES" ]; do
        SERVER_CPUS=0,1
        CLIENT_CPUS=2,3
        CLIENT_THREADS=2
        for line in $two; do
            if available "$line"; then
                suite_run "$line" "pass $pass" "2 server CPUs"
            fi
        done
        SERVER_CPUS=0
        CLIENT_CPUS=1,2,3
        CLIENT_THREADS=3
        for line in $one; do
            if available "$line"; then
                suite_run "$line" "pass $pass" "1 server CPU"
            fi
        done
        pass=$((pass + 1))
    done
    exit 0
fi

lines="reference firn-2 firn-1 $baselines reference-aof firn-aof-2"
for line in $lines; do
    verify "$line"
done
for line in reference firn-2 reference-aof firn-aof-2; do
    verify_expiry "$line"
done
for line in reference-aof firn-aof-2; do
    verify_restart "$line"
done
for line in reference firn-2; do
    verify_idle "$line"
    verify_active "$line"
done
if [ "$MODE" = verify ]; then
    exit 0
fi
round=1
while [ "$round" -le "$ROUNDS" ]; do
    for line in $lines; do
        measure "$line" "$round"
    done
    round=$((round + 1))
done
