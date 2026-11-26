#!/bin/sh
# Check .github/compiler-release-retention.sh on listings that name every
# release kind, and that the selection it replaced would have removed a
# bundled release, or let one take the newest lightweight release's place.
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
script=$here/compiler-release-retention.sh
cutoff=2026-09-20T00:00:00Z
tab=$(printf '\t')

fail() {
    echo "test-compiler-release-retention: $1" >&2
    exit 1
}

# listing "DATE TAG" ... -- the input lines, tab separated.
listing() {
    for line in "$@"; do
        printf '%s\n' "$line" | sed "s/ /$tab/"
    done
}

# expect NAME EXPECTED_OUTPUT LISTING -- compare the script's whole output.
expect() {
    actual=$(printf '%s\n' "$3" | sh "$script" "$cutoff")
    [ "$actual" = "$2" ] || fail "$1: expected
$2
got
$actual"
}

# The selection before bundled releases existed, kept to show the cases below
# separate the two: it removed what is older than the cutoff, except the
# newest release that is not an experiment.
legacy() {
    printf '%s\n' "$1" | awk -F '\t' -v cutoff="$cutoff" '
        $2 !~ /^wf-exp-/ && newest == "" { newest = $2 }
        $1 < cutoff && $2 != newest { print $2 }'
}

# Age removes, and the newest lightweight release stays at any age.
old=$(listing '2026-08-01T00:00:00Z wf-aaaaaaaaaaaa' '2026-07-01T00:00:00Z wf-bbbbbbbbbbbb')
expect 'age and the newest lightweight release' \
"keep${tab}wf-aaaaaaaaaaaa${tab}newest
remove${tab}wf-bbbbbbbbbbbb" "$old"

# A recent release is not removed, however many are newer than it.
recent=$(listing '2026-10-01T00:00:00Z wf-aaaaaaaaaaaa' '2026-09-25T00:00:00Z wf-bbbbbbbbbbbb')
expect 'recent releases' "keep${tab}wf-aaaaaaaaaaaa${tab}newest" "$recent"

# An old bundled release is kept by name and does not take the newest place,
# which stays with the lightweight release a downstream pins; experiment
# releases, bundled or not, go by age.
mixed=$(listing \
    '2026-08-03T00:00:00Z wf-cccccccccccc-bundled' \
    '2026-08-02T00:00:00Z wf-exp-dddddddddddd-bundled' \
    '2026-08-01T00:00:00Z wf-aaaaaaaaaaaa' \
    '2026-07-01T00:00:00Z wf-exp-eeeeeeeeeeee' \
    '2026-06-01T00:00:00Z wf-bbbbbbbbbbbb')
expect 'bundled and experiment releases' \
"keep${tab}wf-cccccccccccc-bundled${tab}bundled
remove${tab}wf-exp-dddddddddddd-bundled
keep${tab}wf-aaaaaaaaaaaa${tab}newest
remove${tab}wf-exp-eeeeeeeeeeee
remove${tab}wf-bbbbbbbbbbbb" "$mixed"

# The previous selection takes the old bundled release as the newest, which
# drops the lightweight release downstreams pin ...
case $(legacy "$mixed") in
    *wf-aaaaaaaaaaaa*) ;;
    *) fail 'the mixed listing no longer separates the legacy selection' ;;
esac
# ... and, once a lightweight release follows it, removes the bundled one.
later=$(listing '2026-10-05T00:00:00Z wf-ffffffffffff' '2026-08-03T00:00:00Z wf-cccccccccccc-bundled')
case $(legacy "$later") in
    *wf-cccccccccccc-bundled*) ;;
    *) fail 'the later listing no longer separates the legacy selection' ;;
esac
expect 'a bundled release outlives a newer lightweight release' \
"keep${tab}wf-ffffffffffff${tab}newest
keep${tab}wf-cccccccccccc-bundled${tab}bundled" "$later"

echo 'test-compiler-release-retention: ok'
