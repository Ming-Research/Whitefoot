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
#   sh redis-bench.sh compare     prebuilt firn images against each other:
#                                 IMAGES lists name=path pairs, one image may
#                                 appear under two names as a noise control;
#                                 each pass starts every image in turn, the
#                                 order reversed on even passes, and runs
#                                 COMPARE_TESTS (default mset set get) at each
#                                 depth in COMPARE_DEPTHS (default 16 1) for
#                                 COMPARE_SECONDS (default 10) each, on
#                                 each server CPU count in COMPARE_CPUS (default 1 2), for
#                                 COMPARE_PASSES passes (default 6); with PERF
#                                 naming a perf executable it then records a
#                                 flat profile of each image under
#                                 COMPARE_PROFILE (default mset at depth 16)
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
    cpus=$(cpu_count "$SERVER_CPUS")
    if [ -z "$KEEP" ]; then
        rm -rf "$OUT/appendonlydir" "$OUT/firn.aof"
    fi
    case $1 in
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
        image-*)
            WF_DRIVERS=$(cpu_count "$SERVER_CPUS") taskset -c "$SERVER_CPUS" \
                "$(image_path "${1#image-}")" "$PORT" 0 - "${IDLE:-0}" \
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
}

fail() {
    echo "$1 failed the correctness pass: $2" >&2
    exit 1
}

# Correct: no error reply in a mixed run, and every one of 100,000 increments
# from 50 clients reaches the one counter.
verify() {
    start "$1"
    taskset -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" -t incr -n 100000 \
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
    taskset -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" -c 50 -n 20000 \
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

# The path IMAGES gives a compare line's name.
image_path() {
    for pair in $IMAGES; do
        if [ "${pair%%=*}" = "$1" ]; then
            echo "${pair#*=}"
            return
        fi
    done
    echo "no image named $1" >&2
    exit 1
}

# The server's CPU time so far in clock ticks, user and system together.
server_ticks() {
    sed 's/^.*) //' "/proc/$server/stat" | awk '{ print $12 + $13 }'
}

# One redis-benchmark run of <test> at depth <depth> for <requests> against
# the started server, its client threads on CLIENT_CPUS; prints the rate over
# the run's wall time, the server's CPU microseconds per request, and the
# client's p50 and p99. redis-benchmark's own rate divides by a clock that
# ticks every 250 ms, a step of 5% in a five-second run, so it is not used.
compare_client() {
    ticks=$(server_ticks)
    begin=$(date +%s%N)
    taskset -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" --threads "$CLIENT_THREADS" \
        -c 50 -n "$3" -r 100000 -d 3 -P "$2" -t "$1" --csv >"$OUT/compare-client.csv" 2>"$OUT/compare-client.err"
    end=$(date +%s%N)
    ticks=$(($(server_ticks) - ticks))
    # The fields between the first and last quoted ones hold no quote.
    awk -F'","' -v n="$3" -v ns=$((end - begin)) -v ticks="$ticks" -v hz="$(getconf CLK_TCK)" '
        $1 != "\"test" {
            printf "%.0f,%.3f,%s,%s\n", n / (ns / 1e9), ticks / hz * 1e6 / n, $5, $7
        }' "$OUT/compare-client.csv"
}

# The comparison of prebuilt images. A line whose replies fail verify stops
# the run; every sample is kept as compare.csv's line,pass,cpus,test,depth,
# requests,rps,server_cpu_us_per_request,p50_ms,p99_ms, and each test's
# request count is sized from the
# fastest image's pilot so that every image runs the same count.
if [ "$MODE" = compare ]; then
    total=$(nproc)
    tests=${COMPARE_TESTS:-mset set get}
    depths=${COMPARE_DEPTHS:-16 1}
    names=
    for pair in $IMAGES; do
        names="$names ${pair%%=*}"
        sha256sum "${pair#*=}" | sed "s/^/image,${pair%%=*},/"
    done
    reversed=$(echo $names | tr ' ' '\n' | awk '{ a[NR] = $0 } END { for (i = NR; i > 0; i--) printf "%s ", a[i] }')
    echo 'line,pass,cpus,test,depth,requests,rps,server_cpu_us_per_request,p50_ms,p99_ms' >"$OUT/compare.csv"
    for n in ${COMPARE_CPUS:-1 2}; do
        if [ "$n" -ge "$total" ]; then
            echo "skip,$n server CPUs,only $total on this host"
            continue
        fi
        SERVER_CPUS=$(seq -s, 0 $((n - 1)))
        CLIENT_THREADS=$((total - n < 16 ? total - n : 16))
        CLIENT_CPUS=$(seq -s, "$n" $((n + CLIENT_THREADS - 1)))
        for name in $names; do
            verify "image-$name"
        done
        sizes="$OUT/compare-sizes-$n.txt"
        : >"$sizes"
        for test in $tests; do
            for depth in $depths; do
                fastest=0
                for name in $names; do
                    start "image-$name"
                    rate=$(compare_client "$test" "$depth" 1000000 | cut -d, -f1)
                    stop
                    fastest=$(awk -v a="$fastest" -v b="$rate" 'BEGIN { print (b > a ? b : a) }')
                done
                echo "$test $depth $(awk -v r="$fastest" -v s="${COMPARE_SECONDS:-10}" 'BEGIN { printf "%d", r * s }')" >>"$sizes"
            done
        done
        pass=1
        while [ "$pass" -le "${COMPARE_PASSES:-6}" ]; do
            order=$names
            if [ $((pass % 2)) -eq 0 ]; then
                order=$reversed
            fi
            for name in $order; do
                start "image-$name"
                while read -r test depth requests; do
                    echo "$name,$pass,$n,$test,$depth,$requests,$(compare_client "$test" "$depth" "$requests")" |
                        tee -a "$OUT/compare.csv"
                done <"$sizes"
                stop
            done
            pass=$((pass + 1))
        done
        if [ -n "$PERF" ]; then
            set -- ${COMPARE_PROFILE:-mset 16}
            requests=$(awk -v t="$1" -v d="$2" '$1 == t && $2 == d { print $3 }' "$sizes")
            for name in $names; do
                start "image-$name"
                "$PERF" record -F 4999 -p "$server" -o "$OUT/perf-$name-$n.data" >/dev/null 2>&1 &
                recorder=$!
                sleep 1
                compare_client "$1" "$2" "$requests" | sed "s/^/profiled,$name,$n,/"
                kill -INT "$recorder"
                wait "$recorder" || true
                stop
                "$PERF" report -i "$OUT/perf-$name-$n.data" --stdio --no-children \
                    --sort symbol --percent-limit 0.3 >"$OUT/profile-$name-$n.txt" 2>/dev/null
            done
        fi
    done
    cat "$OUT/compare.csv"
    exit 0
fi

# The quick comparison, for the loop of changing firn and measuring again:
# firn on QUICK_CPUS server CPUs against the fastest of Garnet and Dragonfly,
# which led every test of the scaling run. Those two are measured once per
# CPU count and client count and kept in quick-ref-<n>-<clients>.csv until
# QUICK_REFRESH is set; nothing
# is verified. Its rates are its own: its client is not the suite's.
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
