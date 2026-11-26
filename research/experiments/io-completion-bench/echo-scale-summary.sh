#!/bin/sh
# The verdict of the TCP echo core-count sweep (linux-net-bench.sh scale): reads
# its raw samples, rejects what is not evidence, and judges each doubling of
# the server's cores.
#
#   sh echo-scale-summary.sh RAW.tsv
#
# One row of RAW.tsv is one recorded pass of one line at one core count,
# tab separated, a dash where a field is missing:
#
#   line cores pass rt_per_s p50_us p99_us connect_us client_busy_max_pct
#   drivers server_exit load_exit server_cpus
#
# `drivers` is the number of driver lines the Whitefoot server reported at
# exit (a dash for the C references), `client_busy_max_pct` the busiest client
# CPU's share of the exchange phase (netload), and the two exits the server's
# and the load generator's.
#
# A pass is rejected, and the run is invalid, when
#   - the server or the load generator exited nonzero,
#   - it has no rate, or no readable client busy share,
#   - its busiest client CPU was busier than WF_SCALE_BUSY_LIMIT percent,
#   - the subject line (wf) reported other than its core count in drivers
#     (a single core may report none or one: the runtime prints nothing when
#     only one driver ran and nothing was handed over),
# and a cell is rejected when fewer than WF_SCALE_MIN_PASSES passes of it
# remain. Nothing is judged from a rejected pass.
#
# Over valid cells, the rate of a cell is the median of its passes, with the
# minimum and maximum beside it, and each line's consecutive core counts give
# a ratio, normalized to one doubling. The subject's doubling is
#   reached          at least WF_SCALE_STEP times,
#   harness-limited  below it, and so is the control line's (uring) at the same
#                    step: the host or the client limits the curve,
#   miss             below it while the control reached it.
#
# Exit status: 0 every doubling of the subject reached, 2 invalid input,
# 3 harness-limited without a miss, 4 a miss.
#
# The thresholds are the research record's, set before the first measured run
# (research/investigations/echo-scaling/DESIGN.md).

set -e

RAW=${1:?usage: echo-scale-summary.sh RAW.tsv}
STEP=${WF_SCALE_STEP:-1.6}
BUSY_LIMIT=${WF_SCALE_BUSY_LIMIT:-80}
MIN_PASSES=${WF_SCALE_MIN_PASSES:-5}
SUBJECT=${WF_SCALE_SUBJECT:-wf}
CONTROL=${WF_SCALE_CONTROL:-uring}

status=0
awk -F'\t' -v step="$STEP" -v limit="$BUSY_LIMIT" -v minimum="$MIN_PASSES" \
    -v subject="$SUBJECT" -v control="$CONTROL" '
function numeric(text) { return text ~ /^-?[0-9]+(\.[0-9]+)?$/ }
function median(line, cores,    k, i, j, keep, count, key) {
    key = line SUBSEP cores
    count = passes[key]
    for (i = 0; i < count; i++) sorted[i] = value[key, i]
    for (i = 0; i < count; i++)
        for (j = i + 1; j < count; j++)
            if (sorted[j] < sorted[i]) { keep = sorted[i]; sorted[i] = sorted[j]; sorted[j] = keep }
    if (count % 2 == 1) return sorted[(count - 1) / 2]
    return (sorted[count / 2 - 1] + sorted[count / 2]) / 2
}
{
    if (NF != 12) {
        problems++
        print "invalid: row " NR " has " NF " fields, not 12"
        next
    }
    line = $1; cores = $2 + 0; pass = $3
    where = "line=" line " cores=" cores " pass=" pass
    reasons = ""
    if ($10 != "0") reasons = reasons " server exit " $10 ";"
    if ($11 != "0") reasons = reasons " load generator exit " $11 ";"
    if (!numeric($4) || $4 + 0 <= 0) reasons = reasons " no rate;"
    if (!numeric($8) || $8 + 0 < 0) reasons = reasons " client busy share unreadable;"
    else if ($8 + 0 > limit + 0) reasons = reasons " busiest client CPU " $8 "% is over " limit "%;"
    if (line == subject) {
        expected_low = cores; expected_high = cores
        if (cores == 1) expected_low = 0
        if (!numeric($9) || $9 + 0 < expected_low || $9 + 0 > expected_high)
            reasons = reasons " ran " $9 " drivers, not " cores ";"
    }
    seen_line[line] = 1
    seen_cores[line SUBSEP cores] = 1
    cell_cores[cores] = 1
    if (reasons != "") {
        problems++
        print "invalid: " where ":" reasons
        next
    }
    key = line SUBSEP cores
    value[key, passes[key]] = $4 + 0
    passes[key]++
    if (!(key in lowest) || $4 + 0 < lowest[key]) lowest[key] = $4 + 0
    if (!(key in highest) || $4 + 0 > highest[key]) highest[key] = $4 + 0
}
END {
    if (!(subject in seen_line)) { print "invalid: no " subject " line in the samples"; problems++ }
    if (!(control in seen_line)) { print "invalid: no " control " control line in the samples"; problems++ }
    for (key in seen_cores) {
        split(key, parts, SUBSEP)
        if (passes[key] + 0 < minimum + 0) {
            print "invalid: line=" parts[1] " cores=" parts[2] " has " (passes[key] + 0) \
                " valid passes, fewer than " minimum
            problems++
        }
    }

    n = 0
    for (c in cell_cores) order[n++] = c + 0
    for (i = 0; i < n; i++)
        for (j = i + 1; j < n; j++)
            if (order[j] < order[i]) { keep = order[i]; order[i] = order[j]; order[j] = keep }

    printf "\n%-10s %6s %7s %14s %14s %14s\n", "line", "cores", "passes", "median_rt_s", "min", "max"
    m = 0
    for (l in seen_line) names[m++] = l
    for (i = 0; i < m; i++)
        for (j = i + 1; j < m; j++)
            if (names[j] < names[i]) { keep = names[i]; names[i] = names[j]; names[j] = keep }
    for (i = 0; i < m; i++) {
        for (k = 0; k < n; k++) {
            key = names[i] SUBSEP order[k]
            if (!(key in seen_cores)) continue
            if (passes[key] > 0) {
                med[key] = median(names[i], order[k])
                printf "%-10s %6d %7d %14.1f %14.1f %14.1f\n", names[i], order[k], passes[key], med[key], lowest[key], highest[key]
            } else {
                printf "%-10s %6d %7d %14s %14s %14s\n", names[i], order[k], 0, "-", "-", "-"
            }
        }
    }

    printf "\n%-10s %-9s %9s %12s\n", "line", "cores", "ratio", "per_doubling"
    for (i = 0; i < m; i++) {
        for (k = 0; k + 1 < n; k++) {
            a = names[i] SUBSEP order[k]; b = names[i] SUBSEP order[k + 1]
            if (!(a in med) || !(b in med) || med[a] <= 0) continue
            ratio = med[b] / med[a]
            per = exp(log(ratio) * log(2) / log(order[k + 1] / order[k]))
            doubling[names[i], k] = per
            printf "%-10s %2d -> %-3d %9.2f %12.2f\n", names[i], order[k], order[k + 1], ratio, per
        }
    }

    printf "\n%-10s %6s %12s\n", "line", "cores", subject "/line"
    for (i = 0; i < m; i++) {
        if (names[i] == subject) continue
        for (k = 0; k < n; k++) {
            a = subject SUBSEP order[k]; b = names[i] SUBSEP order[k]
            if ((a in med) && (b in med) && med[b] > 0)
                printf "%-10s %6d %12.2f\n", names[i], order[k], med[a] / med[b]
        }
    }

    misses = 0; limited = 0; reached = 0
    print ""
    for (k = 0; k + 1 < n; k++) {
        if (!((subject, k) in doubling)) continue
        s = doubling[subject, k]
        c = ((control, k) in doubling) ? doubling[control, k] : -1
        if (s >= step + 0) { verdict = "reached"; reached++ }
        else if (c >= 0 && c < step + 0) { verdict = "harness-limited"; limited++ }
        else { verdict = "miss"; misses++ }
        printf "doubling %d -> %d: %s (%s %.2f per doubling, %s %.2f, step %s)\n", order[k], order[k + 1], verdict, subject, s, control, c, step
    }
    if (problems > 0) {
        print "RESULT: invalid (" problems " problems above)"
        exit 2
    }
    if (reached + limited + misses == 0) { print "RESULT: invalid (no doubling to judge)"; exit 2 }
    if (misses > 0) { print "RESULT: miss"; exit 4 }
    if (limited > 0) { print "RESULT: harness-limited"; exit 3 }
    print "RESULT: reached"
    exit 0
}' "$RAW" || status=$?
exit "$status"
