#!/bin/sh
# Experiment 7 of research/investigations/io-model/SHARED.md: the Redis
# subset `tests/programs/redis_subset.wf` against `redis-server` with
# persistence off, driven by `redis-benchmark`. It builds the subset with the
# worktree's compiler, checks both servers for correctness, then measures
# them interleaved and prints one CSV line per run.
#
#   sh redis-bench.sh             the correctness pass and the protocol
#   sh redis-bench.sh verify      only the correctness pass
#
# The servers run pinned to SERVER_CPUS and the client to CLIENT_CPUS, so the
# two never share a core; nothing else should run on the host meanwhile.
set -e

ROOT=${ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
OUT=${OUT:-/tmp/redis-bench}
WHITEFOOTC=${WHITEFOOTC:-$ROOT/compiler/target/debug/whitefootc}
SERVER_CPUS=${SERVER_CPUS:-0,1}
CLIENT_CPUS=${CLIENT_CPUS:-2,3}
REQUESTS=${REQUESTS:-1000000}
ROUNDS=${ROUNDS:-2}
PORT=${PORT:-17379}
MODE=${1:-bench}

mkdir -p "$OUT"
"$WHITEFOOTC" -o "$OUT/redis_subset" "$ROOT/tests/programs/redis_subset.wf"

server=
# Each start takes a fresh port: a stopped server's accepted connections wait
# out TIME_WAIT on its port, and the subset's runtime does not set
# SO_REUSEADDR.
start() {
    PORT=$((PORT + 1))
    case $1 in
        reference)
            taskset -c "$SERVER_CPUS" redis-server --port "$PORT" --save "" \
                --appendonly no --daemonize no >"$OUT/reference.log" 2>&1 &
            ;;
        subset-*)
            WF_DRIVERS=${1#subset-} taskset -c "$SERVER_CPUS" \
                "$OUT/redis_subset" "$PORT" 0 >"$OUT/subset.log" 2>&1 &
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
        echo "$1 failed the correctness pass" >&2
        exit 1
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

for line in reference subset-2 subset-1; do
    verify "$line"
done
if [ "$MODE" = verify ]; then
    exit 0
fi
round=1
while [ "$round" -le "$ROUNDS" ]; do
    for line in reference subset-2 subset-1; do
        measure "$line" "$round"
    done
    round=$((round + 1))
done
