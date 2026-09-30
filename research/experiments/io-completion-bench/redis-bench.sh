#!/bin/sh
# Experiments 7 and 8 of the io-model investigation: the Redis subset
# `tests/programs/redis_subset.wf` against `redis-server`, driven by
# `redis-benchmark`. Experiment 7 (SHARED.md) runs both with persistence
# off; Experiment 8 (TIME-AND-FILES.md) adds both with an append-only file
# synced every second, and the subset of the revision before expiry. It
# builds the subset with the worktree's compiler, checks every line for
# correctness, then measures them interleaved and prints one CSV line per
# run.
#
#   sh redis-bench.sh             the correctness pass and the protocol
#   sh redis-bench.sh verify      only the correctness pass
#
# BASELINE_ROOT, when set, is a worktree of the revision before expiry with
# its compiler built; its subset is measured as the baseline lines.
#
# The servers run pinned to SERVER_CPUS and the client to CLIENT_CPUS, so the
# two never share a core; nothing else should run on the host meanwhile.
set -e

ROOT=${ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
OUT=${OUT:-/tmp/redis-bench}
WHITEFOOTC=${WHITEFOOTC:-$ROOT/compiler/target/debug/whitefootc}
BASELINE_ROOT=${BASELINE_ROOT:-}
SERVER_CPUS=${SERVER_CPUS:-0,1}
CLIENT_CPUS=${CLIENT_CPUS:-2,3}
REQUESTS=${REQUESTS:-1000000}
ROUNDS=${ROUNDS:-2}
# A server that closes an idle client first holds that port in TIME_WAIT for
# a minute, and the subset's runtime does not set SO_REUSEADDR, so each run
# starts its ports from its own process number rather than from one fixed
# port a run a minute earlier may still hold.
PORT=${PORT:-$((10000 + $$ % 400 * 50))}
MODE=${1:-bench}

mkdir -p "$OUT"
"$WHITEFOOTC" -o "$OUT/redis_subset" "$ROOT/tests/programs/redis_subset.wf"
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
# Each start takes a fresh port: a stopped server's accepted connections wait
# out TIME_WAIT on its port, and the subset's runtime does not set
# SO_REUSEADDR.
start() {
    PORT=$((PORT + 1))
    if [ -z "$KEEP" ]; then
        rm -rf "$OUT/appendonlydir" "$OUT/subset.aof"
    fi
    case $1 in
        reference)
            taskset -c "$SERVER_CPUS" redis-server --port "$PORT" --save "" \
                --appendonly no --timeout "${IDLE:-0}" --daemonize no \
                >"$OUT/reference.log" 2>&1 &
            ;;
        reference-aof)
            taskset -c "$SERVER_CPUS" redis-server --port "$PORT" --save "" \
                --appendonly yes --appendfsync everysec --dir "$OUT" \
                --timeout "${IDLE:-0}" --daemonize no \
                >"$OUT/reference.log" 2>&1 &
            ;;
        subset-aof-*)
            (cd "$OUT" && WF_DRIVERS=${1##*-} exec taskset -c "$SERVER_CPUS" \
                ./redis_subset "$PORT" 0 subset.aof "${IDLE:-0}") \
                >"$OUT/subset.log" 2>&1 &
            ;;
        subset-*)
            WF_DRIVERS=${1#subset-} taskset -c "$SERVER_CPUS" \
                "$OUT/redis_subset" "$PORT" 0 - "${IDLE:-0}" \
                >"$OUT/subset.log" 2>&1 &
            ;;
        baseline-*)
            WF_DRIVERS=${1#baseline-} taskset -c "$SERVER_CPUS" \
                "$OUT/redis_baseline" "$PORT" 0 >"$OUT/subset.log" 2>&1 &
            ;;
    esac
    server=$!
    tries=0
    until redis-cli -p "$PORT" PING 2>/dev/null | grep -q PONG; do
        tries=$((tries + 1))
        if [ "$tries" -gt 200 ]; then
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
# The subset stops by being killed, so the check waits 100 milliseconds for
# its writer, which appends every 10, before stopping it; Redis flushes its
# file when it is stopped.
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

measure() {
    start "$1"
    for pipeline in 1 16; do
        taskset -c "$CLIENT_CPUS" redis-benchmark -p "$PORT" --threads 2 \
            -c 50 -n "$REQUESTS" -r 100000 -d 16 -t set,get -P "$pipeline" \
            --csv 2>/dev/null | grep -v '^"test"' |
            sed "s/^/$1,round $2,pipeline $pipeline,/"
    done
    stop
}

lines="reference subset-2 subset-1 $baselines reference-aof subset-aof-2"
for line in $lines; do
    verify "$line"
done
for line in reference subset-2 reference-aof subset-aof-2; do
    verify_expiry "$line"
done
for line in reference-aof subset-aof-2; do
    verify_restart "$line"
done
for line in reference subset-2; do
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
