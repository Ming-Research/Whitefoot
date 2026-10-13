#!/bin/sh
# Feed .github/gate-both-platforms.sh job listings through a stand-in `gh` and
# check its verdict and how many times it listed. The stand-in answers the
# runs query with one run id and the jobs query with the fixture of the
# current listing (jobs.N, the last fixture once N runs past them), as gh's
# --jq would print it; it answers only the queries the script must make
# (workflow gate.yml, the head_sha, filter=latest, --paginate). FAKE_FAIL_FIRST
# makes that many calls fail before it answers, and FAKE_FAIL_ALWAYS makes
# every call fail. Waits are zero, so each case takes a moment.
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
script=$here/gate-both-platforms.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-gate-both.XXXXXX")
trap 'rm -rf "$work"' EXIT

mkdir "$work/bin"
cat > "$work/bin/gh" <<'STANDIN'
#!/bin/sh
echo "$*" >> "$FAKE_DIR/calls"
n=$(wc -l < "$FAKE_DIR/calls" | tr -d ' ')
if [ -n "${FAKE_FAIL_ALWAYS:-}" ] || [ "$n" -le "${FAKE_FAIL_FIRST:-0}" ]; then
    echo "gh: HTTP 502 (stand-in)" >&2
    exit 1
fi
case "$*" in
    *--paginate*) ;;
    *) echo "gh: stand-in needs --paginate: $*" >&2; exit 2 ;;
esac
case "$*" in
    *repos/org/repo/actions/workflows/gate.yml/runs\?head_sha=abc123\&*)
        echo "$n" >> "$FAKE_DIR/runs-listings"
        echo 101
        ;;
    *repos/org/repo/actions/runs/101/jobs\?filter=latest\&*)
        k=$(wc -l < "$FAKE_DIR/runs-listings" | tr -d ' ')
        [ -f "$FAKE_DIR/jobs.$k" ] || k=$(ls "$FAKE_DIR" | sed -n 's/^jobs\.//p' | sort -n | tail -1)
        cat "$FAKE_DIR/jobs.$k"
        ;;
    *) echo "gh: unexpected query: $*" >&2; exit 2 ;;
esac
STANDIN
chmod +x "$work/bin/gh"

groups='["static","unit","corpus"]'

# jobs FILE [NAME=CONCLUSION ...] -- write a listing with every job green,
# except the named ones; CONCLUSION `absent` leaves the job out.
jobs() {
    file=$1
    shift
    : > "$file"
    for g in static unit corpus; do
        for os in ubuntu-24.04 macos-15; do
            name="$g ($os)"
            conclusion=success
            for override in "$@"; do
                [ "${override%%=*}" != "$name" ] || conclusion=${override#*=}
            done
            [ "$conclusion" = absent ] || printf '2026-10-13T10:00:00Z\t%s\t%s\n' "$name" "$conclusion" >> "$file"
        done
    done
}

fail() {
    {
        echo "test-gate-both-platforms: $1"
        echo '--- output:'; cat "$work/out"
        echo '--- stderr:'; cat "$work/err"
        echo '--- gh calls:'; cat "$FAKE_DIR/calls"
    } >&2
    exit 1
}

# gate [ENV=VALUE ...] -- run the script on the fixtures in $FAKE_DIR; sets
# status and listings (the number of runs queries).
gate() {
    status=0
    env FAKE_DIR="$FAKE_DIR" PATH="$work/bin:$PATH" GH_TOKEN=x HEAD_SHA=abc123 \
        GATE_GROUPS="$groups" GITHUB_REPOSITORY=org/repo RUNNER_TEMP="$work" \
        GATE_RETRY_WAIT=0 GATE_RELIST_WAIT=0 "$@" \
        sh "$script" > "$work/out" 2> "$work/err" || status=$?
    listings=0
    [ ! -f "$FAKE_DIR/runs-listings" ] || listings=$(wc -l < "$FAKE_DIR/runs-listings" | tr -d ' ')
}

# newcase NAME -- a fresh fixture directory with no fixtures and no calls.
newcase() {
    FAKE_DIR=$work/case-$1
    rm -rf "$FAKE_DIR"
    mkdir "$FAKE_DIR"
    : > "$FAKE_DIR/calls"
    export FAKE_DIR
}

want() { # NAME PATTERN -- the output holds a line matching PATTERN
    grep -q -- "$2" "$work/out" "$work/err" || fail "$1: expected output matching '$2'"
}

# (a) All green on the first try: pass with one listing.
newcase a
jobs "$FAKE_DIR/jobs.1"
gate
[ "$status" -eq 0 ] || fail 'a: expected a pass'
[ "$listings" -eq 1 ] || fail "a: expected 1 listing, got $listings"
want a '^ok      corpus (macos-15)$'

# (b) gh fails twice, then answers: the calls are retried.
newcase b
jobs "$FAKE_DIR/jobs.1"
gate FAKE_FAIL_FIRST=2
[ "$status" -eq 0 ] || fail 'b: expected a pass after two failed calls'
want b 'gh api call failed (try 2 of 5)'
want b '^ok      static (ubuntu-24.04)$'

# (c) The first listing lacks macOS unit and corpus; the second is complete.
newcase c
jobs "$FAKE_DIR/jobs.1" 'unit (macos-15)=absent' 'corpus (macos-15)=absent'
jobs "$FAKE_DIR/jobs.2"
gate
[ "$status" -eq 0 ] || fail 'c: expected a pass after the second listing'
[ "$listings" -eq 2 ] || fail "c: expected 2 listings, got $listings"
want c '^MISSING unit (macos-15): no gate job$'
want c '^ok      unit (macos-15)$'

# (d) A job failed: fail at once, with one listing, and still print every line.
newcase d
jobs "$FAKE_DIR/jobs.1" 'unit (macos-15)=failure' 'corpus (macos-15)=absent'
jobs "$FAKE_DIR/jobs.2"
gate
[ "$status" -ne 0 ] || fail 'd: expected a failure'
[ "$listings" -eq 1 ] || fail "d: expected 1 listing, got $listings"
want d '^MISSING unit (macos-15): failure$'
want d '^MISSING corpus (macos-15): no gate job$'
want d '^ok      static (ubuntu-24.04)$'
want d 'is not green on both platforms'

# (e) A job never appears: fail after 4 listings.
newcase e
jobs "$FAKE_DIR/jobs.1" 'corpus (macos-15)=absent'
gate
[ "$status" -ne 0 ] || fail 'e: expected a failure'
[ "$listings" -eq 4 ] || fail "e: expected 4 listings, got $listings"
want e '^MISSING corpus (macos-15): no gate job$'

# (f) gh always fails: fail after 5 tries, naming the call.
newcase f
jobs "$FAKE_DIR/jobs.1"
gate FAKE_FAIL_ALWAYS=1
[ "$status" -ne 0 ] || fail 'f: expected a failure'
[ "$(wc -l < "$FAKE_DIR/calls" | tr -d ' ')" -eq 5 ] || fail 'f: expected 5 tries'
want f '::error::gh api call failed 5 times: list gate.yml runs for abc123'

# (g) The latest completed job of a name decides, whichever way round: a later
# success supersedes an earlier failure, and a later failure an earlier
# success, as a re-run or a later run of the same revision does.
newcase g1
jobs "$FAKE_DIR/jobs.1" 'unit (macos-15)=cancelled'
printf '2026-10-13T11:00:00Z\tunit (macos-15)\tsuccess\n' >> "$FAKE_DIR/jobs.1"
gate
[ "$status" -eq 0 ] || fail 'g1: expected the later success to decide'
want g1 '^ok      unit (macos-15)$'
newcase g2
jobs "$FAKE_DIR/jobs.1"
printf '2026-10-13T11:00:00Z\tunit (macos-15)\tfailure\n' >> "$FAKE_DIR/jobs.1"
gate
[ "$status" -ne 0 ] || fail 'g2: expected the later failure to decide'
want g2 '^MISSING unit (macos-15): failure$'

echo 'test-gate-both-platforms: ok'
