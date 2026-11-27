#!/bin/sh
# Decide whether a run of sanitizers.yml or io-hosts.yml checks its revision,
# from what the revision changes. Prints `needed=` and `why=` lines for
# $GITHUB_OUTPUT, after .github/gate-scope.sh has decided by the event that
# the run checks.
#
# A revision on main is checked whole: a push to main, the nightly run and a
# manual run on main. Any other revision is a change: what differs between
# its merge base with origin/main and itself. The workflow checks a change
# only where the change touches a path that the workflow's jobs read, and
# says so where it does not (AGENTS.md, "Branch and main boundary", item 4).
# What a change left unchecked, the nightly run on main checks.
#
# The table below names what each workflow's jobs read. An entry names a file
# or a directory and everything under it. A changed path takes the row of the
# longest entry that is the path itself or a directory above it, and counts
# where that row says `read` for the workflow. A path that no row names counts
# as read, and the reason names it.
# The diff lists a renamed file at both its names, so that moving a file out
# of what the jobs read counts as touching it.
#
# Usage, in a clone of the revision that has origin/main and the history of
# both, as a CI checkout with fetch-depth 0 has:
#   change-scope.sh WORKFLOW                 decide for HEAD
#   change-scope.sh --decide WORKFLOW BASE   decide for the changed paths on
#                                            standard input, one a line, from
#                                            BASE
#   change-scope.sh --rows                   the table, without its comments
# WORKFLOW is a column of the table: sanitizers or io-hosts.
#
# The script prints the changed paths it counted, each with its row, on
# standard error. .github/test-change-scope.sh exercises it and holds the
# table to what the jobs read.
set -eu
LC_ALL=C
export LC_ALL

rows() {
    sed -e 's/[[:space:]]*#.*//' -e '/^$/d' <<'EOF'
path                                      sanitizers  io-hosts
#
# The compiler package, which every job of both workflows builds with its
# corpus test, and what the package reads outside itself as it builds and as
# that test runs. The test-change-scope.sh check finds these reads in the
# package's sources; it cannot tell which of the corpus test's tests a job
# runs, so both workflows read all of them.
compiler                                  read        read
lib                                       read        read
spec/kernel-spec.md                       read        read
tests/conformance                         read        read
tests/programs                            read        read
tests/package-fixtures                    read        read
apps                                      read        read
docs/guide                                read        read
README.md                                 read        read
README.zh-CN.md                           read        read
# A checkout writes each .wf, .wfm and .wfg file byte for byte by this file.
.gitattributes                            read        read
#
# Each workflow's own file, the scripts its steps run and the data they read.
.github/workflows/sanitizers.yml          read        -
.github/workflows/io-hosts.yml            -           read
.github/gate-scope.sh                     read        read
.github/change-scope.sh                   read        read
.github/llvm-major                        read        read
.github/run-check.pl                      read        read
.github/time-budgets.txt                  read        read
.github/junit.py                          read        read
.github/toolchain/fetch-llvm-mingw.sh     -           read
.github/toolchain/llvm-assets.json        -           read
#
# What no job of either workflow reads.
.github                                   -           -
.gitignore                                -           -
.gitmodules                               -           -
AGENTS.md                                 -           -
LICENSE                                   -           -
Makefile                                  -           -
archive                                   -           -
design                                    -           -
docs                                      -           -
governance                                -           -
proof                                     -           -
research                                  -           -
spec                                      -           -
tests                                     -           -
wf-agents.toml                            -           -
EOF
}

fail() {
    printf 'change-scope: %s\n' "$*" >&2
    exit 1
}

# decide WORKFLOW BASE: the decision for the changed paths on standard input.
decide() {
    TABLE=$(rows) awk -v workflow="$1" -v base="$2" '
        function short(commit) { return substr(commit, 1, 12) }
        BEGIN {
            lines = split(ENVIRON["TABLE"], line, "\n")
            for (i = 1; i <= lines; i++) {
                fields = split(line[i], field)
                if (i == 1) {
                    for (c = 2; c <= fields; c++)
                        if (field[c] == workflow)
                            column = c
                    if (!column) {
                        printf "change-scope: the table has no column for the workflow \"%s\"\n", workflow > "/dev/stderr"
                        broken = 1
                        exit
                    }
                    width = fields
                    continue
                }
                if (fields != width || (field[column] != "read" && field[column] != "-")) {
                    printf "change-scope: the row \"%s\" is not a path and a mark, read or -, for each workflow\n", line[i] > "/dev/stderr"
                    broken = 1
                    exit
                }
                count++
                entry[count] = field[1]
                mark[count] = field[column]
            }
        }
        $0 != "" {
            changed++
            best = 0
            for (r = 1; r <= count; r++) {
                e = entry[r]
                if ($0 == e || substr($0, 1, length(e) + 1) == e "/")
                    if (!best || length(e) > length(entry[best]))
                        best = r
            }
            if (!best) {
                printf "  %s: no row\n", $0 > "/dev/stderr"
                if (!unnamed) unnamed = $0
                reads++
                if (!first) first = $0
            } else {
                printf "  %s: row %s, %s\n", $0, entry[best], mark[best] > "/dev/stderr"
                if (mark[best] == "read") {
                    reads++
                    if (!first) first = $0
                }
            }
        }
        END {
            if (broken)
                exit 2
            name = workflow ".yml"
            if (unnamed)
                printf "needed=true\nwhy=%s checks the change from %s: it touches %s, which no row of .github/change-scope.sh names\n", name, short(base), unnamed
            else if (reads == 1)
                printf "needed=true\nwhy=%s checks the change from %s: it touches %s, which its jobs read\n", name, short(base), first
            else if (reads)
                printf "needed=true\nwhy=%s checks the change from %s: it touches %s and %d more paths its jobs read\n", name, short(base), first, reads - 1
            else
                printf "needed=false\nwhy=%s checks nothing: the change from %s touches none of the paths its jobs read\n", name, short(base)
        }'
}

case "${1:-}" in
    --rows)
        [ "$#" = 1 ] || fail 'usage: change-scope.sh --rows'
        rows
        ;;
    --decide)
        [ "$#" = 3 ] || fail 'usage: change-scope.sh --decide WORKFLOW BASE'
        decide "$2" "$3"
        ;;
    -* | '')
        fail 'usage: change-scope.sh WORKFLOW | --decide WORKFLOW BASE | --rows'
        ;;
    *)
        [ "$#" = 1 ] || fail 'usage: change-scope.sh WORKFLOW'
        workflow=$1
        main=$(git rev-parse --verify --quiet 'refs/remotes/origin/main^{commit}') ||
            fail 'refs/remotes/origin/main does not exist; check out with fetch-depth 0'
        head=$(git rev-parse --verify --quiet 'HEAD^{commit}') ||
            fail 'HEAD is not a commit'
        if git merge-base --is-ancestor "$head" "$main"; then
            # The table is read here too, so a broken table fails every run.
            decide "$workflow" "$head" < /dev/null > /dev/null
            printf 'needed=true\nwhy=%s.yml checks %s whole, a revision on main\n' \
                "$workflow" "$(printf '%.12s' "$head")"
            exit 0
        fi
        base=$(git merge-base "$main" "$head") ||
            fail "origin/main $(printf '%.12s' "$main") and HEAD $(printf '%.12s' "$head") have no merge base"
        changes=$(git -c core.quotePath=false diff --name-only --no-renames "$base" "$head") ||
            fail "cannot list the change from $(printf '%.12s' "$base")"
        printf 'change-scope: the change from %s to %s, for %s.yml:\n' \
            "$(printf '%.12s' "$base")" "$(printf '%.12s' "$head")" "$workflow" >&2
        printf '%s\n' "$changes" | decide "$workflow" "$base"
        ;;
esac
