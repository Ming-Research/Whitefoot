#!/bin/sh
# Test .github/change-scope.sh: the decision it makes for changed paths and for
# commits, and its table against what the jobs of sanitizers.yml and
# io-hosts.yml read, so that a path the jobs read cannot be left out of the
# table unseen.
#
# What the jobs read is found in the files themselves:
# - every tracked file has a row of the table, and every row names a tracked
#   file or a directory that holds one;
# - a workflow's own file, every repository path it names outside its
#   comments, and every file of their directories that a file so named names
#   outside its comments, is read by that workflow;
# - every repository path outside compiler/ that the compiler package names,
#   which every job of both workflows builds with its corpus test, is read by
#   both: a `../` path of compiler/Makefile, a string of a Rust or C source
#   that leaves the package with `../`, and a chain of `.join("...")` of a
#   source that names a path of the repository.
# Every failure is reported before the test ends.
set -eu
LC_ALL=C
export LC_ALL
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
script=$here/change-scope.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-change-scope.XXXXXX")
trap 'rm -rf "$work"' EXIT
failures=0
workflows='sanitizers io-hosts'
base=0123456789abcdef0123456789abcdef01234567

fail() {
    printf 'change-scope: %s\n' "$1" >&2
    if [ -s "$work/out" ]; then
        echo '--- decision:' >&2
        cat "$work/out" >&2
    fi
    failures=$((failures + 1))
}

# decide WORKFLOW PATH...: the decision for these changed paths.
decide() {
    workflow=$1
    shift
    printf '%s\n' "$@" | sh "$script" --decide "$workflow" "$base" > "$work/out" 2> "$work/err"
}

# expect NAME NEEDED WHY-FRAGMENT: after a decision that succeeded.
expect() {
    if ! grep -qx "needed=$2" "$work/out"; then
        fail "$1: expected needed=$2"
    elif [ "$(grep -c '' "$work/out")" != 2 ] || ! grep -q '^why=' "$work/out"; then
        fail "$1: expected only a needed= and a why= line"
    elif ! grep -qF -- "$3" "$work/out"; then
        fail "$1: expected a reason saying '$3'"
    fi
}

# --- The decision for changed paths.

for workflow in $workflows; do
    decide "$workflow" docs/ideas.md research/README.md design/decisions/compiler.md AGENTS.md
    expect "$workflow, prose only" false "$workflow.yml checks nothing: the change from 0123456789ab touches none of the paths its jobs read"
    decide "$workflow"
    expect "$workflow, no change" false 'touches none of the paths its jobs read'
    decide "$workflow" compiler/src/backend/heap.c
    expect "$workflow, a runtime unit" true "$workflow.yml checks the change from 0123456789ab: it touches compiler/src/backend/heap.c, which its jobs read"
    decide "$workflow" docs/ideas.md compiler/src/backend/heap.c tests/programs/a.wf lib/std/x.wf
    expect "$workflow, several reads" true 'it touches compiler/src/backend/heap.c and 2 more paths its jobs read'
    # The longest entry decides, and an entry is a whole path or a whole
    # directory.
    decide "$workflow" spec/kernel-spec.md
    expect "$workflow, the specification" true 'touches spec/kernel-spec.md'
    decide "$workflow" spec/log.md spec/kernel-spec-v0.1.md docs/articles/a.md .github/test-run-check.sh
    expect "$workflow, beside what is read" false 'touches none'
    decide "$workflow" docs/guide/a.md
    expect "$workflow, the guide" true 'touches docs/guide/a.md'
    # A path no row names counts as read, and the reason names it.
    for path in elsewhere/a.md compilers/a.rs compiler README; do
        decide "$workflow" docs/ideas.md "$path"
        expect "$workflow, unnamed $path" true "it touches $path, which no row of .github/change-scope.sh names"
    done
done
decide sanitizers .github/workflows/io-hosts.yml .github/toolchain/llvm-assets.json
expect 'sanitizers, the files of the I/O hosts' false 'checks nothing'
decide io-hosts .github/workflows/io-hosts.yml
expect 'io-hosts, its own file' true 'touches .github/workflows/io-hosts.yml'
decide io-hosts .github/workflows/sanitizers.yml
expect 'io-hosts, the file of the sanitizers' false 'checks nothing'
decide sanitizers .github/workflows/sanitizers.yml
expect 'sanitizers, its own file' true 'touches .github/workflows/sanitizers.yml'

# The table names a path and the two workflows, and each row decides for a
# path under it as its mark says.
: > "$work/out"
sh "$script" --rows > "$work/rows" || fail 'the table could not be printed'
header=$(sed -n 1p "$work/rows" | awk '{ $1 = $1; print }')
[ "$header" = 'path sanitizers io-hosts' ] ||
    fail "the table's first line names the path and the columns sanitizers and io-hosts: $header"
for workflow in $workflows; do
    # The loop runs in a subshell, so its failures reach the count by a file.
    sed 1d "$work/rows" | while read -r entry sanitizers io_hosts; do
        case "$workflow" in
            sanitizers) mark=$sanitizers ;;
            io-hosts) mark=$io_hosts ;;
        esac
        case "$entry" in
            */) path=${entry}a-file ;;
            *) path=$entry ;;
        esac
        decide "$workflow" "$path"
        case "$mark" in
            read) expect "$workflow, the row $entry" true "$path" ;;
            *) expect "$workflow, the row $entry" false 'checks nothing' ;;
        esac
    done 2> "$work/row-failures"
    if [ -s "$work/row-failures" ]; then
        cat "$work/row-failures" >&2
        failures=$((failures + 1))
    fi
done

: > "$work/out"
if printf 'a\n' | sh "$script" --decide windows "$base" > "$work/out" 2> "$work/err"; then
    fail 'an unknown workflow was decided'
elif ! grep -q 'no column for the workflow "windows"' "$work/err"; then
    fail 'an unknown workflow: wrong refusal'
fi

# --- The decision for commits, in a repository of its own.

repo=$work/repo
git -c init.defaultBranch=main init -q "$repo"
git_in() {
    git -C "$repo" -c user.name=test -c user.email=test@example.invalid -c commit.gpgsign=false "$@"
}
commit() { # commit MESSAGE: commits everything
    git_in add -A
    git_in commit -q -m "$1"
}
scope() { # scope WORKFLOW: the decision for HEAD
    (cd "$repo" && sh "$script" "$1") > "$work/out" 2> "$work/err"
}
mkdir -p "$repo/compiler/src" "$repo/docs"
echo a > "$repo/compiler/src/a.c"
echo a > "$repo/docs/a.md"
commit 'main'
fork=$(git_in rev-parse HEAD)
: > "$work/out"
if scope sanitizers; then
    fail 'no origin/main: the decision did not fail'
elif ! grep -q 'refs/remotes/origin/main does not exist' "$work/err"; then
    fail 'no origin/main: wrong refusal'
fi
git_in update-ref refs/remotes/origin/main "$fork"
scope sanitizers || fail 'main itself: the decision failed'
expect 'main itself' true "sanitizers.yml checks $(printf '%.12s' "$fork") whole, a revision on main"
git_in checkout -q -b topic
echo b > "$repo/docs/a.md"
commit 'docs'
scope sanitizers || fail 'a change of prose: the decision failed'
expect 'a change of prose' false "the change from $(printf '%.12s' "$fork") touches none"
# Main moves on with a change the jobs read; the branch is charged only with
# its own change, before it merges main and after.
git_in checkout -q --detach "$fork"
echo b > "$repo/compiler/src/b.c"
commit 'main moves on'
moved=$(git_in rev-parse HEAD)
git_in update-ref refs/remotes/origin/main "$moved"
git_in checkout -q topic
scope io-hosts || fail 'behind main: the decision failed'
expect 'behind main' false "the change from $(printf '%.12s' "$fork") touches none"
git_in merge -q --no-edit "$moved"
scope io-hosts || fail 'main merged: the decision failed'
expect 'main merged' false "the change from $(printf '%.12s' "$moved") touches none"
# A file moved out of what the jobs read counts at its old name.
mkdir -p "$repo/docs/old"
git_in mv compiler/src/a.c docs/old/a.c
commit 'move'
scope sanitizers || fail 'a move out of the compiler: the decision failed'
expect 'a move out of the compiler' true 'it touches compiler/src/a.c, which its jobs read'
grep -q '^  docs/old/a.c: row docs/, -$' "$work/err" ||
    fail 'a move out of the compiler: its new name was not listed'

# --- The table against the repository.

cd "$root"
git ls-files > "$work/tracked"
top=$(git ls-tree --name-only HEAD | tr '\n' ' ')

# Every row names something tracked.
sed 1d "$work/rows" | while read -r entry rest; do
    case "$entry" in
        */) awk -v entry="$entry" 'index($0, entry) == 1 { found = 1; exit } END { exit !found }' "$work/tracked" ||
            echo "the row $entry names no directory that holds a tracked file" ;;
        *) grep -qxF -- "$entry" "$work/tracked" ||
            echo "the row $entry names no tracked file" ;;
    esac
done > "$work/stale"
if [ -s "$work/stale" ]; then
    sed 's/^/change-scope: /' "$work/stale" >&2
    failures=$((failures + 1))
fi

# Every tracked file has a row.
: > "$work/out"
sh "$script" --decide sanitizers "$base" < "$work/tracked" > /dev/null 2> "$work/err" ||
    fail 'the tracked files could not be decided'
if grep -q ': no row$' "$work/err"; then
    grep ': no row$' "$work/err" |
        sed 's/^ */change-scope: a tracked file has no row of the table: /; s/: no row$//' >&2
    failures=$((failures + 1))
fi

# reads WORKFLOW FROM < PATHS: each path is read by the workflow by a row that
# names it or a directory above it; FROM says where the paths were found. A
# path that names a directory of tracked files is that directory, with its `/`.
reads() {
    : > "$work/out"
    awk -v tracked="$work/tracked" '
        BEGIN {
            while ((getline line < tracked) > 0)
                while (sub(/\/[^\/]*$/, "", line)) dir[line] = 1
        }
        { sub(/\/$/, ""); print (($0 in dir) ? $0 "/" : $0) }
    ' | sort -u > "$work/found"
    [ -s "$work/found" ] || return 0
    if ! sh "$script" --decide "$1" "$base" < "$work/found" > /dev/null 2> "$work/err"; then
        fail "$2: the paths could not be decided"
        return 0
    fi
    grep '^  ' "$work/err" | grep -v ', read$' | sed 's/^  //; s/: [^:]*$//' > "$work/unread" || true
    while read -r path; do
        fail "$2 names $path, which no row says $1.yml reads"
    done < "$work/unread"
}

# The repository paths in a file's lines that are not comments: what stands
# before `.github/` and the leading `../` left out.
named_in() { # named_in FILE
    grep -v -E '^[[:space:]]*#' "$1" |
        grep -oE '[A-Za-z0-9_.-]+(/[A-Za-z0-9_.-]+)+' |
        sed -E 's#^(\.\./)+##; s#^.*/(\.github/)#\1#' |
        while read -r path; do
            case " $top " in
                *" ${path%%/*} "*) printf '%s\n' "$path" ;;
            esac
        done
}

for workflow in $workflows; do
    file=.github/workflows/$workflow.yml
    { printf '%s\n' "$file"; named_in "$file"; } | sort -u > "$work/named"
    reads "$workflow" "$file" < "$work/named"
    # The files beside each tracked file so named that it names.
    while read -r named; do
        grep -qxF -- "$named" "$work/tracked" || continue
        grep -v -E '^[[:space:]]*#' "$named" > "$work/text" || true
        awk -v directory="${named%/*}/" 'index($0, directory) == 1 && index(substr($0, length(directory) + 1), "/") == 0' "$work/tracked" |
            while read -r sibling; do
                if grep -qF -- "${sibling##*/}" "$work/text"; then
                    printf '%s\n' "$sibling"
                fi
            done
    done < "$work/named" > "$work/siblings"
    reads "$workflow" "a file $file runs or reads" < "$work/siblings"
    grep -qE "change-scope\\.sh $workflow([^a-z-]|\$)" "$file" ||
        fail "$file does not decide by its own column of the table, change-scope.sh $workflow"
    grep -q '^  schedule:$' "$file" ||
        fail "$file has no nightly run on main"
done

# What the compiler package names outside itself.
grep -v -E '^[[:space:]]*#' compiler/Makefile |
    grep -oE "(^|[[:space:](=\"'])\\.\\./[A-Za-z0-9_./-]+" |
    sed -E "s#^[[:space:](=\"']?##; s#^(\\.\\./)+##" > "$work/package"
grep -E '^compiler/.*\.(rs|c|h)$' "$work/tracked" > "$work/sources"
# Each source is read line by line: a chain of `.join` may go on over the
# next lines, each beginning with `.join`.
# shellcheck disable=SC2046
awk -v tracked="$work/tracked" -v top="$top" '
    # The path a relative path names from a directory, or "" above the root.
    function resolve(directory, relative,   parts, n, i, out, m, path) {
        m = split(directory, out, "/")
        n = split(relative, parts, "/")
        for (i = 1; i <= n; i++) {
            if (parts[i] == "" || parts[i] == ".") continue
            if (parts[i] == "..") { if (m == 0) return ""; m--; continue }
            out[++m] = parts[i]
        }
        path = ""
        for (i = 1; i <= m; i++) path = path (i > 1 ? "/" : "") out[i]
        return path
    }
    function exists(path) { return path != "" && (path in file || path in dir) }
    # Says a path of the repository outside the package.
    function outside(path,   first) {
        if (path == "" || path ~ /^compiler(\/|$)/) return
        first = path
        sub(/\/.*/, "", first)
        if (first in toplevel) print path
    }
    # A chain names the repository path where the repository has it and the
    # package has no path of that name, or has one but the chain names more
    # than one level; a single name the package has too is the package own.
    function chain(path) {
        if (!exists(path)) return
        if (exists("compiler/" path) && path !~ /\//) return
        outside(path)
    }
    function components(text) {
        gsub(/\.join\("/, "", text)
        gsub(/"\)[ \t]*/, "/", text)
        sub(/\/$/, "", text)
        return text
    }
    BEGIN {
        while ((getline line < tracked) > 0) {
            file[line] = 1
            while (sub(/\/[^\/]*$/, "", line)) dir[line] = 1
        }
        n = split(top, names, " ")
        for (i = 1; i <= n; i++) toplevel[names[i]] = 1
        JOINS = "\\.join\\(\"[^\"]*\"\\)([ \t]*\\.join\\(\"[^\"]*\"\\))*"
    }
    FNR == 1 {
        if (pending != "") chain(pending)
        pending = ""
        here = FILENAME
        sub(/\/[^\/]*$/, "", here)
    }
    /^[ \t]*\/\// { next }
    {
        rest = $0
        while (match(rest, /"\/?(\.\.\/)+[^"]*"/)) {
            string = substr(rest, RSTART + 1, RLENGTH - 2)
            rest = substr(rest, RSTART + RLENGTH)
            if (string ~ /^\//) {
                outside(resolve("compiler", string))
                continue
            }
            near = resolve(here, string)
            if (exists(near)) outside(near)
            else outside(resolve("compiler", string))
        }
        if (FILENAME !~ /\.rs$/) next
        rest = $0
        sub(/^[ \t]*/, "", rest)
        if (pending != "") {
            if (match(rest, "^" JOINS)) {
                pending = pending "/" components(substr(rest, 1, RLENGTH))
                rest = substr(rest, RLENGTH + 1)
                if (rest ~ /^[ \t]*$/) next
            }
            chain(pending)
            pending = ""
        }
        while (match(rest, JOINS)) {
            found = components(substr(rest, RSTART, RLENGTH))
            rest = substr(rest, RSTART + RLENGTH)
            if (rest ~ /^[ \t]*$/) pending = found
            else chain(found)
        }
    }
    END { if (pending != "") chain(pending) }
' $(cat "$work/sources") >> "$work/package"
for workflow in $workflows; do
    reads "$workflow" 'the compiler package' < "$work/package"
done

if [ "$failures" -gt 0 ]; then
    echo "change-scope: $failures failures" >&2
    exit 1
fi
echo 'change-scope: every decision as expected, and the table holds every path the jobs read'
