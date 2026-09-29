#!/bin/sh
# The TCP echo protocol of io-completion-bench. It builds the compiler and the
# three C tools from one worktree, builds the Whitefoot server from the same
# compiler, and prints one table: for every connection count and every server
# line, the median round-trip rate, latency and connect time over the recorded
# passes, and the Whitefoot line's ratio to each reference.
#
# Two hosts run it and neither is privileged over the other: this repository's
# `linux-net` target runs it here with the Makefile's paths, and the project's
# continuous-integration runner exports ROOT, OUT, CLANG and CARGO_TARGET_DIR
# and runs the same bytes. Only the paths differ.
#
#   sh linux-net-bench.sh          build everything and run the protocol
#   sh linux-net-bench.sh verify   only the correctness pass, over binaries
#                                  another build already put in $OUT
#   sh linux-net-bench.sh measure  the correctness pass and the protocol over
#                                  binaries another build already put in $OUT
#   sh linux-net-bench.sh memory   the resident memory and mapping count each
#                                  idle connection adds to every Whitefoot line
#                                  in $OUT (WAITS.md, Experiment 3)
#
# The bar this measures against is the one
# research/investigations/io-model/NETWORK.md section 6 sets: the reference is
# the fastest existing solution regardless of language, which on a Linux
# loopback is a hand-written C server on io_uring using what the kernel offers
# for exactly this shape. `uring_echo` is that server, `epoll_echo` is the
# second reference, and Whitefoot's number is a ratio to them. The gap is the
# result, not something to hide.
set -e

ROUNDS=${ROUNDS:-5}
WARMUP=${WARMUP:-1}

ROOT=${ROOT:-/work}
BUNDLE=$ROOT/research/experiments/io-completion-bench
OUT=${OUT:-/scratch/io-net-bench}
CLANG=${CLANG:-/usr/bin/clang}
MODE=${1:-bench}

# The plan. One line per case: label suffix, connections, round trips per
# connection, message bytes.
#
# The round-trip counts are chosen so one run is roughly a second at 64
# connections on a four-core host, and so the three counts do comparable total
# work: 20000 round trips on the single connection, 2000 on each of 64, 200 on
# each of 1024. The fourth case is the bytes-per-second one: 64 KiB messages,
# 200 round trips per connection over 64 connections.
CASES="k1 1 20000 64
k64 64 2000 64
k1024 1024 200 64
k64.64k 64 200 65536"

# The server lines to run, out of "uring epoll wf wf1 wfbase waiting", space
# separated. `wf1` is the same binary as `wf` run with one driver thread.
# `wfbase` is a second Whitefoot build of the same source, from an earlier
# compiler, put in $OUT as wf_echo_base by whoever compares the two. The
# default is every line whose binary is in $OUT, which is every line the build
# above produced. Naming a subset is for the case where one server cannot
# complete a run yet and the others still owe a table; the table says which
# lines it holds.
NET_LINES=${NET_LINES:-}

# The Whitefoot lines' environments: the shipped defaults, and one driver.
WF_ENVIRONMENT=""
WF1_ENVIRONMENT="WF_DRIVERS=1"

# --- the pieces a run is made of ----------------------------------------

# The window a server's port is drawn from: below the kernel's ephemeral
# range, because the load generator's own thousand connections take their
# local ports out of that range and a listener cannot bind a port one of them
# already holds.
PORT_CEILING=$(awk '{print $1}' /proc/sys/net/ipv4/ip_local_port_range 2>/dev/null)
PORT_CEILING=${PORT_CEILING:-32768}
PORT_FLOOR=10000
if [ "$PORT_CEILING" -le $((PORT_FLOOR + 1000)) ]; then
    PORT_FLOOR=1024
fi
PORT_SPAN=$((PORT_CEILING - PORT_FLOOR))

# A port nothing is listening on. Checked against the kernel's own table
# rather than by connecting: every connect is one of the server's counted
# connections, so a probe connection would change the workload it is probing.
free_port() {
    while :; do
        candidate=$(od -An -N2 -tu2 < /dev/urandom | tr -d ' ')
        candidate=$((PORT_FLOOR + candidate % PORT_SPAN))
        if ! port_is_listening "$candidate"; then
            echo "$candidate"
            return 0
        fi
    done
}

port_is_listening() {
    hex=$(printf '%04X' "$1")
    tables=/proc/net/tcp
    if [ -r /proc/net/tcp6 ]; then
        tables="$tables /proc/net/tcp6"
    fi
    awk -v hex="$hex" '
        $4 == "0A" {
            if (split($2, parts, ":") == 2 && parts[2] "" == hex "") { found = 1 }
        }
        END { exit found ? 0 : 1 }' $tables 2>/dev/null
}

# Waits for the server to be reachable, and for nothing else. There is no
# timeout here and no sleep chosen to be "long enough": the loop ends when the
# listening socket exists or when the server process is gone, and the second
# case prints what the server said.
wait_for_listener() {
    port=$1
    pid=$2
    label=$3
    while :; do
        if port_is_listening "$port"; then
            return 0
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "$label: the server exited before it listened on $port" >&2
            cat "$OUT/server.err" >&2
            exit 1
        fi
        sleep 0.01
    done
}

# One measured run: a fresh port, a server started for exactly K connections,
# the load generator against it, and the server's own exit status.
run_case() {
    label=$1
    binary=$2
    environment=$3
    connections=$4
    roundtrips=$5
    bytes=$6
    recording=$7

    port=$(free_port)
    if [ -n "$environment" ]; then
        env $environment "$binary" "$port" "$connections" \
            >"$OUT/server.out" 2>"$OUT/server.err" &
    else
        "$binary" "$port" "$connections" >"$OUT/server.out" 2>"$OUT/server.err" &
    fi
    server=$!
    wait_for_listener "$port" "$server" "$label"

    measured=0
    line=$("$OUT/netload" "$port" "$connections" "$roundtrips" "$bytes") || measured=$?
    if [ "$measured" != 0 ]; then
        echo "$label: the load generator failed" >&2
        cat "$OUT/server.err" >&2
        wait "$server" 2>/dev/null || true
        exit 1
    fi

    status=0
    wait "$server" || status=$?
    if [ "$status" != 0 ]; then
        echo "$label: the server exited with status $status" >&2
        cat "$OUT/server.err" >&2
        exit 1
    fi
    if [ -s "$OUT/server.err" ]; then
        echo "$label: the server wrote to its diagnostic channel:" >&2
        cat "$OUT/server.err" >&2
    fi

    if [ "$recording" = 1 ]; then
        printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$label" \
            "$(field "$line" rt_per_s)" "$(field "$line" p50_us)" \
            "$(field "$line" p99_us)" "$(field "$line" connect_us)" \
            "$(field "$line" bytes_per_s)" >> "$OUT/raw.tsv"
    fi
}

field() {
    echo "$1" | tr '\t' '\n' | sed -n "s/^$2=//p"
}

line_environment() {
    case $1 in
        wf) echo "$WF_ENVIRONMENT" ;;
        wf1) echo "$WF1_ENVIRONMENT" ;;
        *) echo "" ;;
    esac
}

median_of() {
    awk -F'\t' -v want="$1" -v column="$2" '
        $1 == want { values[count++] = $column + 0 }
        END {
            if (count == 0) { printf "-"; exit }
            for (i = 0; i < count; i++) {
                for (j = i + 1; j < count; j++) {
                    if (values[j] < values[i]) {
                        keep = values[i]; values[i] = values[j]; values[j] = keep
                    }
                }
            }
            if (count % 2 == 1) { printf "%.1f", values[(count - 1) / 2] }
            else { printf "%.1f", (values[count / 2 - 1] + values[count / 2]) / 2 }
        }' "$OUT/raw.tsv"
}

ratio_of() {
    awk -v candidate="$1" -v reference="$2" '
        BEGIN {
            if (candidate + 0 == 0 || reference + 0 == 0) { printf "-" }
            else { printf "%.2f", candidate / reference }
        }'
}

# --- the build ------------------------------------------------------------

if [ "$MODE" = bench ]; then
    cd "$ROOT/compiler"
    cargo build --profile gate --bin whitefootc --locked --offline
    WFC=${CARGO_TARGET_DIR:-$ROOT/compiler/target}/gate/whitefootc

    rm -rf "$OUT"
    mkdir -p "$OUT"
    cd "$BUNDLE"
    "$CLANG" -std=c11 -O2 -Wall -Wextra -Werror -pthread uring_echo.c -o "$OUT/uring_echo"
    "$CLANG" -std=c11 -O2 -Wall -Wextra -Werror -pthread epoll_echo.c -o "$OUT/epoll_echo"
    "$CLANG" -std=c11 -O2 -Wall -Wextra -Werror -pthread netload.c -o "$OUT/netload"
    "$CLANG" -std=c11 -O2 -Wall -Wextra -Werror -pthread waiting_echo.c -o "$OUT/waiting_echo"

    # The maintained context server: one context per connection [WAIT-3].
    "$WFC" -o "$OUT/wf_echo" "$ROOT/tests/programs/tcp_contexts.wf"
fi

if [ ! -x "$OUT/netload" ]; then
    echo "linux-net-bench: $OUT/netload is not built" >&2
    exit 1
fi

LINES=""
for name in ${NET_LINES:-uring epoll wf wf1 wfbase waiting}; do
    case $name in
        uring) binary=$OUT/uring_echo ;;
        epoll) binary=$OUT/epoll_echo ;;
        wf|wf1) binary=$OUT/wf_echo ;;
        wfbase) binary=$OUT/wf_echo_base ;;
        waiting) binary=$OUT/waiting_echo ;;
        *) echo "linux-net-bench: there is no $name line" >&2; exit 2 ;;
    esac
    if [ -x "$binary" ]; then
        LINES="$LINES$name $binary
"
    elif [ -n "$NET_LINES" ]; then
        echo "linux-net-bench: $binary is not built" >&2
        exit 1
    elif [ "$name" != wfbase ]; then
        echo "note: $binary was not built, so the table is without the $name line."
    fi
done

# --- idle-connection memory -----------------------------------------------
#
# Each Whitefoot line is started for N connections, `idleload` opens them and
# sends nothing, and reads what they added to the server's resident set and
# mapping count once the server has accepted every one; closing them lets the
# server exit, which it must do with status zero. N is bounded by the host's
# descriptor limit, which `idleload` and the server each need N of.

if [ "$MODE" = memory ]; then
    if [ ! -x "$OUT/idleload" ]; then
        echo "linux-net-bench: $OUT/idleload is not built" >&2
        exit 1
    fi
    for count in ${MEMORY_COUNTS:-1000 5000 19000}; do
        echo "$LINES" | while read -r name binary; do
            case $name in wf|wf1|wfbase) ;; *) continue ;; esac
            port=$(free_port)
            env $(line_environment "$name") "$binary" "$port" "$count" \
                >"$OUT/server.out" 2>"$OUT/server.err" &
            server=$!
            wait_for_listener "$port" "$server" "$name.memory"
            line=$("$OUT/idleload" "$port" "$count" "$server")
            status=0
            wait "$server" || status=$?
            if [ "$status" != 0 ]; then
                echo "$name.memory: the server exited with status $status" >&2
                cat "$OUT/server.err" >&2
                exit 1
            fi
            printf '%s\t%s\n' "$name" "$line"
        done
    done
    exit 0
fi

# --- the correctness pass -------------------------------------------------
#
# Every server answers the load generator before any of them reports a time.
# The generator compares every echoed byte with the byte it sent, so a server
# that publishes the wrong bytes fails here rather than reporting a fast time.

echo "$LINES" | while read -r name binary; do
    [ -n "$name" ] || continue
    environment=$(line_environment "$name")
    run_case "$name.verify" "$binary" "$environment" 4 200 64 0
done
echo "every server echoes what netload sent, at 4 connections"

if [ "$MODE" = verify ]; then
    exit 0
fi

# --- the timed passes -----------------------------------------------------
#
# A pass is every line of every case once. WARMUP passes are unrecorded, then
# ROUNDS recorded ones, and alternate passes run the plan in reverse, for the
# reason runner.c states: a shared host drifts over the minutes a table takes,
# and a grouped schedule turns that drift into a difference between lines.

: > "$OUT/plan.txt"
echo "$CASES" | while read -r suffix connections roundtrips bytes; do
    [ -n "$suffix" ] || continue
    echo "$LINES" | while read -r name binary; do
        [ -n "$name" ] || continue
        printf '%s.%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$suffix" "$name" "$binary" \
            "$connections" "$roundtrips" "$bytes" >> "$OUT/plan.txt"
    done
done
sed '1!G;h;$!d' "$OUT/plan.txt" > "$OUT/plan-reversed.txt"

: > "$OUT/raw.tsv"
passes=$((WARMUP + ROUNDS))
pass=0
while [ "$pass" -lt "$passes" ]; do
    recording=0
    order="$OUT/plan.txt"
    direction="plan order"
    if [ "$pass" -ge "$WARMUP" ]; then
        recording=1
    fi
    if [ $((pass % 2)) = 1 ]; then
        order="$OUT/plan-reversed.txt"
        direction="reversed"
    fi
    if [ "$recording" = 1 ]; then
        echo "pass $((pass + 1)) of $passes ($direction)" >&2
    else
        echo "pass $((pass + 1)) of $passes ($direction, warm-up)" >&2
    fi
    while IFS='	' read -r label name binary connections roundtrips bytes; do
        [ -n "$label" ] || continue
        environment=$(line_environment "$name")
        run_case "$label" "$binary" "$environment" "$connections" "$roundtrips" "$bytes" \
            "$recording"
    done < "$order"
    pass=$((pass + 1))
done

# --- the table ------------------------------------------------------------

echo
echo "TCP echo, 127.0.0.1, medians of $ROUNDS recorded passes after $WARMUP warm-up"
echo "each server was started for exactly the connection count of its line and"
echo "had to exit zero after every one of those connections closed"
echo
printf '%-18s %6s %7s %8s %12s %10s %10s %12s %10s %10s\n' \
    "line" "conns" "bytes" "trips" "rt_per_s" "p50_us" "p99_us" "connect_us" "vs_uring" "vs_epoll"
echo "$CASES" | while read -r suffix connections roundtrips bytes; do
    [ -n "$suffix" ] || continue
    reference=$(median_of "uring.$suffix" 2)
    portable=$(median_of "epoll.$suffix" 2)
    echo "$LINES" | while read -r name binary; do
        [ -n "$name" ] || continue
        label="$name.$suffix"
        rate=$(median_of "$label" 2)
        printf '%-18s %6s %7s %8s %12s %10s %10s %12s %10s %10s\n' \
            "$label" "$connections" "$bytes" "$roundtrips" \
            "$rate" "$(median_of "$label" 3)" "$(median_of "$label" 4)" \
            "$(median_of "$label" 5)" \
            "$(ratio_of "$rate" "$reference")" "$(ratio_of "$rate" "$portable")"
    done
done
echo
printf '%-18s %14s\n' "line" "bytes_per_s"
echo "$LINES" | while read -r name binary; do
    [ -n "$name" ] || continue
    printf '%-18s %14s\n' "$name.k64.64k" "$(median_of "$name.k64.64k" 6)"
done
echo
echo "raw samples: $OUT/raw.tsv"
