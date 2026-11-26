# The data-placement control's comparison of two arms' 32 positions
# (tests/performance/README.md, "Data placement").
#
#   awk -v stage=first|final|aside -v kernels="a b ..." -f positions.awk LAYOUT ROWS
#
# The kernels are named by the caller (data-placement.sh reads them from the
# Makefile's KERNELS, as compare.sh does). LAYOUT holds one line per kernel
# named, `KERNEL alike|differ`, as data-placement.sh record writes it. ROWS holds `KERNEL ARM POSITION ROUND MEDIAN_NS`, one per
# launch, only for kernels that differ: each position 0 to 31 once for each arm
# in round 1, and in round 2 for exactly the kernels round 1 launches again.
#
# Each arm's 32 times are divided by the arm's median (the mean of its 16th and
# 17th), so a loss the arm has at every position cancels, and sorted; the
# quotient at a rank is the candidate's value over the baseline's. A rank is
# unlike upward at 1.10 or more and downward at 1/1.10 or less, in full
# precision. The comparison does not read which position a time came from:
# under address randomization a launch draws its position, so two arms that
# have their times in equal shares are judged alike.
#
# stage=first prints `KERNEL again` for each kernel with an unlike rank in
# round 1. stage=final prints one line per kernel, `alike`, `judged` or `aside`,
# aside when round 2 has a rank unlike in a direction in which round 1 had one,
# with both arms' sorted quotients for each kernel set aside, and last
# `ASIDE: ...`. stage=aside prints only the names set aside, on one line.
# Anything else in the input is refused (exit 2).
function refuse(message) { print "REFUSED: " message; bad = 1; exit 2 }
function positive(value) { return value ~ /^[0-9]+([.][0-9]*)?([eE][+-]?[0-9]+)?$/ && value + 0 > 0 && value + 0 < 1e100 }
function check_layout(   k) {
    if (layout_checked) return
    layout_checked = 1
    for (k = 1; k <= nk; k++) if (!(names[k] in state)) refuse("the layout lacks " names[k])
}
function complete(kernel, round,   a, p) {
    for (a = 1; a <= 2; a++)
        for (p = 0; p < 32; p++)
            if (!((kernel, arms[a], p, round) in time))
                refuse(kernel " " arms[a] " round " round " lacks position " p)
}
# Reads one round of one kernel: sorted quotients and medians kept for printing,
# the unlike ranks counted by direction.
function compare(kernel, round,   a, p, i, j, n, v, x, middle, q) {
    for (a = 1; a <= 2; a++) {
        for (p = 0; p < 32; p++) v[p + 1] = time[kernel, arms[a], p, round]
        for (i = 2; i <= 32; i++) {
            x = v[i]
            for (j = i - 1; j >= 1 && v[j] > x; j--) v[j + 1] = v[j]
            v[j + 1] = x
        }
        middle = (v[16] + v[17]) / 2
        median[kernel, round, arms[a]] = middle
        for (i = 1; i <= 32; i++) sorted[kernel, round, arms[a], i] = v[i] / middle
    }
    up[kernel, round] = down[kernel, round] = 0
    for (i = 1; i <= 32; i++) {
        q = sorted[kernel, round, "candidate", i] / sorted[kernel, round, "baseline", i]
        if (i == 1 || q < low[kernel, round]) low[kernel, round] = q
        if (i == 1 || q > high[kernel, round]) high[kernel, round] = q
        if (q >= line) up[kernel, round]++
        if (q <= 1 / line) down[kernel, round]++
    }
}
function summary(kernel, round) {
    return sprintf("round %d: %d unlike rank(s), %d up and %d down, quotients %.6f to %.6f",
                   round, up[kernel, round] + down[kernel, round], up[kernel, round],
                   down[kernel, round], low[kernel, round], high[kernel, round])
}
BEGIN {
    nk = split(kernels, names, " ")
    if (nk < 1) refuse("no kernels named")
    for (k = 1; k <= nk; k++) {
        if (names[k] !~ /^[a-z][a-z0-9_]*$/ || names[k] in known) refuse("invalid or repeated kernel name " names[k])
        known[names[k]] = 1
    }
    arms[1] = "baseline"; arms[2] = "candidate"
    line = 1.10
    if (stage != "first" && stage != "final" && stage != "aside") refuse("stage must be first, final or aside")
}
FILENAME == ARGV[1] {
    if (NF != 2 || !($1 in known) || ($2 != "alike" && $2 != "differ"))
        refuse("unreadable layout line " FNR ": " $0)
    if ($1 in state) refuse("the layout names " $1 " twice")
    state[$1] = $2
    next
}
{
    check_layout()
    if (NF != 5 || !($1 in known) || ($2 != "baseline" && $2 != "candidate") ||
        $3 !~ /^[0-9]+$/ || $3 + 0 > 31 || ($4 != "1" && $4 != "2") || !positive($5))
        refuse("unreadable row " FNR ": " $0)
    if (state[$1] != "differ") refuse($1 " maps alike but has a row")
    if ($4 == "2" && stage == "first") refuse("a second round before the first is read")
    id = $1 SUBSEP $2 SUBSEP ($3 + 0) SUBSEP $4
    if (id in time) refuse("duplicate row " $1 " " $2 " " ($3 + 0) " " $4)
    time[id] = $5 + 0
    if ($4 == "2") second[$1] = 1
}
END {
    if (bad) exit 2
    check_layout()
    for (k = 1; k <= nk; k++) {
        kernel = names[k]
        if (state[kernel] != "differ") continue
        complete(kernel, 1)
        compare(kernel, 1)
        again[kernel] = (up[kernel, 1] + down[kernel, 1] > 0)
    }
    if (stage == "first") {
        for (k = 1; k <= nk; k++) if (again[names[k]]) print names[k] " again"
        exit 0
    }
    for (k = 1; k <= nk; k++) {
        kernel = names[k]
        if ((kernel in second) && !again[kernel])
            refuse(kernel " has a second round but was not launched again")
        if (!again[kernel]) continue
        complete(kernel, 2)
        compare(kernel, 2)
        aside[kernel] = (up[kernel, 1] && up[kernel, 2]) || (down[kernel, 1] && down[kernel, 2])
    }
    list = ""
    for (k = 1; k <= nk; k++) if (aside[names[k]]) list = list (list == "" ? "" : " ") names[k]
    if (stage == "aside") { print list; exit 0 }
    print "rule: each arm's 32 positions over its median, sorted, compared rank by rank; a rank is unlike at 1.10 or more"
    print "or 1/1.10 or less; a kernel is set aside when round 2 has a rank unlike in a direction round 1 had one."
    for (k = 1; k <= nk; k++) {
        kernel = names[k]
        if (state[kernel] != "differ") { printf "%-12s alike\n", kernel; continue }
        printf "%-12s %s %s", kernel, (aside[kernel] ? "aside" : "judged"), summary(kernel, 1)
        if (again[kernel]) printf "; %s", summary(kernel, 2)
        printf "\n"
        if (!aside[kernel]) continue
        for (r = 1; r <= 2; r++)
            for (a = 1; a <= 2; a++) {
                printf "%s round %d %s median %.6f ms:", kernel, r, arms[a], median[kernel, r, arms[a]] / 1e6
                for (i = 1; i <= 32; i++) printf " %.3f", sorted[kernel, r, arms[a], i]
                printf "\n"
            }
    }
    print "ASIDE: " (list == "" ? "none" : list)
}
