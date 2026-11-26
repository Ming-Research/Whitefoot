#!/bin/sh
# Check .github/gate-matrix.sh against the real Makefile inventory and against a
# stand-in inventory: which host runs parts and which runs groups whole, the
# names the final job requires, and that every job name is one a file or
# artifact may carry, and that `make check-groups` is still the list of group
# names that main's gate.yml reads when the engine checks a commit with it. It
# runs `make` for the two inventories and jq, nothing heavier. It also reads the
# seams between the Makefile, the time budgets, gate.yml and the adapter's tests
# that the part runner names, with `make` printing the commands it would run
# (the stage runner replaced by `echo`) and starting no stage.
set -eu
# The static stage of a gate job runs with the host's toolchain wait in its
# environment; the checks below name the wait themselves.
unset TOOLCHAIN_READY
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
script=$here/gate-matrix.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-gate-matrix.XXXXXX")
trap 'rm -rf "$work"' EXIT

fail() {
    echo "test-gate-matrix: $1" >&2
    exit 1
}

# output PLATFORMS -- the script's two lines for the platforms, in $work/out.
output() {
    sh "$script" "$1" > "$work/out" || fail "the script failed for $1"
    matrix=$(sed -n 's/^matrix=//p' "$work/out")
    jobs=$(sed -n 's/^jobs=//p' "$work/out")
}

# has JSON JQ-EXPRESSION MESSAGE -- the expression is true of the JSON.
has() {
    [ "$(printf '%s' "$1" | jq "$2")" = true ] || fail "$3"
}

linux='["ubuntu-24.04"]'
macos='["macos-15"]'
both='["ubuntu-24.04","macos-15"]'

# The real inventory: Linux runs every part, macOS each group once, whole.
inventory=$(make -C "$here/.." --no-print-directory --silent check-parts)
has "$inventory" 'length > 0 and all(.[]; (.group | length) > 0 and (.shard | length) > 0)' 'the inventory is empty or malformed'
# The group list is the parts' groups once each, in order, and only names: the
# engine runs main's gate.yml against a commit's Makefile, whose matrix takes
# each element of `check-groups` as a group name.
groups=$(make -C "$here/.." --no-print-directory --silent check-groups)
has "$groups" 'length > 0 and all(.[]; type == "string")' 'check-groups is not a list of group names'
has "$groups" "(. | length) == (. | unique | length) and . == ($inventory | map(.group) | reduce .[] as \$g ([]; if index(\$g) then . else . + [\$g] end))" 'check-groups differs from the groups of check-parts'
# The numbered parts of a group (`1/4`, `walk-2/3`) together cover its tests
# only when they are exactly i/N for i = 1..N with one N for each kind of part,
# so a mistyped or missing number in the Makefile fails here and not by running
# less than the group's tests. Both numbered kinds must exist.
numbered='[.[] | select(.shard | test("^(walk-)?[0-9]+/[0-9]+$"))
    | (.shard | capture("^(?<kind>walk-)?(?<i>[0-9]+)/(?<n>[0-9]+)$")) as $c
    | {kind: (.group + ":" + ($c.kind // "")), i: ($c.i | tonumber), n: ($c.n | tonumber)}]'
has "$inventory" "$numbered | map(.kind) | unique == [\"corpus:walk-\", \"unit:\"]" 'the inventory lacks the unit parts or the corpus walk parts'
has "$inventory" "$numbered | group_by(.kind) | all(.[]; ([.[].n] | unique | length) == 1 and ([.[].i] | sort) == [range(1; .[0].n + 1)])" 'the numbered parts of a group are not exactly 1/N to N/N'
output "$linux"
has "$matrix" "length == ($inventory | length) and all(.[]; .os == \"ubuntu-24.04\")" 'Linux does not run every part of the inventory'
has "$jobs" 'length == (unique | length)' 'two Linux jobs have one name'
has "$jobs" 'all(.[]; contains("/") | not)' 'a job name holds a slash'
has "$jobs" 'index("static (ubuntu-24.04)") != null and index("runtime (ubuntu-24.04)") != null' 'static and runtime are not run on Linux'
has "$jobs" 'map(select(startswith("corpus"))) | length >= 2' 'the corpus group is not split on Linux'
has "$jobs" 'index("corpus receipts (ubuntu-24.04)") != null' 'Linux does not run the whole proof-receipt test'
output "$macos"
has "$matrix" 'all(.[]; .shard == "all" and .name == .group and .os == "macos-15")' 'macOS does not run each group whole'
has "$matrix" "map(.group) == ($inventory | map(.group) | reduce .[] as \$g ([]; if index(\$g) then . else . + [\$g] end))" 'macOS groups are not the inventory groups once, in order'
output "$both"
has "$jobs" 'map(select(endswith("(macos-15)"))) | length >= 1' 'a run asking for macOS has no macOS job'

# A stand-in inventory with one split group between two whole groups, so the
# order and the one-whole-run-per-group rule do not depend on today's groups.
mkdir "$work/bin"
cat > "$work/bin/make" <<'STANDIN'
#!/bin/sh
echo '[{"group":"a","shard":"all","name":"a"},{"group":"b","shard":"1/2","name":"b 1 of 2"},{"group":"b","shard":"2/2","name":"b 2 of 2"},{"group":"c","shard":"all","name":"c"}]'
STANDIN
chmod +x "$work/bin/make"
standin() { # PLATFORMS -- output, with the stand-in make first on the path
    saved=$PATH
    PATH=$work/bin:$PATH
    output "$1"
    PATH=$saved
}
standin "$both"
[ "$jobs" = '["a (ubuntu-24.04)","b 1 of 2 (ubuntu-24.04)","b 2 of 2 (ubuntu-24.04)","c (ubuntu-24.04)","a (macos-15)","b (macos-15)","c (macos-15)"]' ] \
    || fail "stand-in inventory: wrong jobs $jobs"
has "$matrix" 'map(select(.os == "macos-15")) | map(.shard) == ["all","all","all"]' 'stand-in: macOS ran a part'
standin "$macos"
[ "$jobs" = '["a (macos-15)","b (macos-15)","c (macos-15)"]' ] || fail "stand-in macOS: wrong jobs $jobs"

# An empty inventory is no gate: the script must not print an empty matrix.
cat > "$work/bin/make" <<'STANDIN'
#!/bin/sh
echo '[]'
STANDIN
if PATH=$work/bin:$PATH sh "$script" "$linux" > "$work/out" 2>/dev/null; then
    fail 'an empty inventory gave a matrix'
fi

# The root Makefile names parts and no host (acceptance 6): which host runs
# which part is gate-matrix.sh's choice alone.
! grep -nE 'ubuntu-|macos-|windows-|runs-on' "$here/../Makefile" || fail 'the root Makefile names a host or runner'

# What the matrix names must exist where it points. Each of these is the seam
# between two files that the others' self-tests cannot see across, and each
# failure is one that otherwise shows only after a gate run has compiled.
root=$here/..
budgets=$here/time-budgets.txt
workflow=$here/workflows/gate.yml

# gate_make ARGS... -- the root Makefile with the stage runner replaced by
# `echo`, so a recipe prints the stage label and command it would run and
# runs nothing; the output is in $work/make.out and its status is returned.
gate_make() {
    make -C "$root" --no-print-directory "$@" CHECK_RUN=echo > "$work/make.out" 2> "$work/make.err"
}

# budget_row LABEL -- a row of the time budgets with a number in each of the
# four Linux CPU columns, which the gate's verdict needs for every labelled
# stage (a missing row fails the final verdict on every host).
budget_row() {
    awk -v label="$1" '$1 == label && $2 ~ /^[0-9]+$/ && $3 ~ /^[0-9]+$/ && $4 ~ /^[0-9]+$/ && $5 ~ /^[0-9]+$/ { found = 1 } END { exit !found }' "$budgets"
}

# Every part's stage label, and the compiler stage label its tests run under,
# has a budget row; the whole group (`all`) keeps the label the group had.
printf '%s' "$inventory" | jq -r '.[] | "\(.group) \(.shard)"' > "$work/parts"
[ -s "$work/parts" ] || fail 'no part to check the budgets of'
while read -r group shard; do
    gate_make check-group GROUP="$group" SHARD="$shard" || fail "check-group $group $shard failed: $(cat "$work/make.err")"
    label=$(sed -n '1s/^\(check\/[^ ]*\) .*/\1/p' "$work/make.out")
    [ -n "$label" ] || fail "check-group $group $shard printed no stage label: $(cat "$work/make.out")"
    budget_row "$label" || fail "the part $group $shard runs under $label, which has no complete row in time-budgets.txt"
    case $group in
        unit | corpus)
            make -C "$root/compiler" --no-print-directory "test-$group" SHARD="$shard" CHECK_RUN=echo NO_CORE_DUMPS= > "$work/make.out" 2> "$work/make.err" \
                || fail "compiler test-$group $shard failed: $(cat "$work/make.err")"
            compiler_label=$(sed -n 's/^\(compiler\/[^ ]*\) .*/\1/p' "$work/make.out" | head -1)
            [ -n "$compiler_label" ] || fail "compiler test-$group $shard printed no stage label: $(cat "$work/make.out")"
            budget_row "$compiler_label" || fail "the part $group $shard runs its tests under $compiler_label, which has no complete row in time-budgets.txt"
            ;;
    esac
done < "$work/parts"
# A whole group still runs under the label it always had, and the parts of a
# kind share one label, so a part is not budgeted as a whole group.
gate_make check-group GROUP=unit || fail 'check-group unit failed'
[ "$(sed -n '1s/ .*//p' "$work/make.out")" = check/unit ] || fail "a whole unit group runs under $(sed -n '1p' "$work/make.out")"
gate_make check-group GROUP=corpus SHARD=walk-2/3 || fail 'check-group corpus walk-2/3 failed'
[ "$(sed -n '1s/ .*//p' "$work/make.out")" = check/corpus-walk ] || fail "a walk part runs under $(sed -n '1p' "$work/make.out")"
grep -q 'SHARD=walk-2/3' "$work/make.out" || fail 'check-group does not hand its part to the group recipe'

# A part the group does not have, a list of parts, an empty part and a group
# with no parts are refused before any stage starts and print no label.
for pair in 'unit 0/4' 'unit 5/4' 'unit walk-1/3' 'unit receipts' 'unit 1/3' 'corpus 1/4' 'corpus walk-4/3' 'corpus walk-1/4' \
    'corpus walk-1/3,receipts' 'corpus rest/3' 'static 1/4' 'runtime all2' 'unit 1/4 2/4'; do
    group=${pair%% *}
    shard=${pair#* }
    if gate_make check-group GROUP="$group" SHARD="$shard"; then
        fail "check-group accepted the part '$shard' of $group"
    fi
    grep -q 'SHARD must be' "$work/make.err" || fail "check-group refused '$shard' of $group for another reason: $(cat "$work/make.err")"
    [ ! -s "$work/make.out" ] || fail "check-group printed a stage for the refused part '$shard' of $group"
done
if gate_make check-group GROUP=unit SHARD=; then
    fail 'check-group took an empty part for the whole group'
fi
if gate_make check-group GROUP='unit corpus'; then
    fail 'check-group accepted two groups'
fi

# The toolchain wait runs where a group's tests begin if the group compiles
# first, and before its stage starts otherwise (acceptance 11): the build comes
# first, then the wait, then the tests. `make -n` prints the commands.
for group in unit corpus; do
    if [ "$group" = unit ]; then shard=2/4; else shard=walk-2/3; fi
    make -n -C "$root" --no-print-directory "_check-$group" SHARD="$shard" CHECK_RUN=echo NO_CORE_DUMPS= TOOLCHAIN_READY='echo TOOLCHAIN-WAIT' > "$work/make.out" 2> "$work/make.err" \
        || fail "a dry run of _check-$group failed: $(cat "$work/make.err")"
    build=$(grep -n "compiler/test-build-$group" "$work/make.out" | head -1 | cut -d: -f1)
    waits=$(grep -c 'TOOLCHAIN-WAIT' "$work/make.out" || true)
    wait_at=$(grep -n 'TOOLCHAIN-WAIT' "$work/make.out" | head -1 | cut -d: -f1)
    tests=$(grep -n "shard-tests.pl $group" "$work/make.out" | head -1 | cut -d: -f1)
    [ -n "$build" ] && [ -n "$wait_at" ] && [ -n "$tests" ] || fail "_check-$group lacks its build, its wait or its tests: $(cat "$work/make.out")"
    [ "$waits" = 1 ] || fail "_check-$group waits for the toolchain $waits times"
    [ "$build" -lt "$wait_at" ] && [ "$wait_at" -lt "$tests" ] || fail "_check-$group does not run build, wait, tests in that order"
    gate_make check-group GROUP="$group" SHARD="$shard" TOOLCHAIN_READY='echo TOOLCHAIN-WAIT' || fail "check-group $group failed with a wait"
    ! grep -q 'TOOLCHAIN-WAIT' "$work/make.out" || fail "check-group $group waits before its stage, which compiles first"
done
for group in static runtime; do
    gate_make check-group GROUP="$group" TOOLCHAIN_READY='echo TOOLCHAIN-WAIT' || fail "check-group $group failed with a wait"
    [ "$(sed -n '1p' "$work/make.out")" = TOOLCHAIN-WAIT ] && sed -n '2p' "$work/make.out" | grep -q "^check/$group " \
        || fail "check-group $group does not wait before its stage starts: $(cat "$work/make.out")"
done
gate_make check-group GROUP=runtime || fail 'the default toolchain wait is not a no-op'

# The workflow reads the matrix by the fields the script writes, and names its
# jobs as the script names them (gate-both-platforms.sh and compiler-release.yml
# look for `<name> (<os>)` and a `(macos-15)` ending).
output "$both"
has "$matrix" 'all(.[]; (keys | sort) == ["group","name","os","shard"])' 'a matrix entry lacks one of group, name, os and shard, or has another field'
grep -v '^ *#' "$workflow" | grep -oE '(^|[^-A-Za-z_./])matrix\.[a-z_]+' | sed 's/.*matrix\.//' | sort -u > "$work/fields"
for field in $(cat "$work/fields"); do
    printf '%s' "$matrix" | jq -e --arg field "$field" 'all(.[]; has($field))' > /dev/null \
        || fail "gate.yml reads matrix.$field, which the matrix entries do not hold"
done
for field in name group shard os; do
    grep -qx "$field" "$work/fields" || fail "gate.yml never reads matrix.$field"
done
awk '/^  gate:$/ { job = 1; next } job && /^  [a-z-]+:$/ { job = 0 } job && /^    name: / { print; exit }' "$workflow" > "$work/job-name"
case $(cat "$work/job-name") in
    '    name: ${{ matrix.name }} (${{ matrix.os }})'*) : ;;
    *) fail "the gate job's name is not '<name> (<os>)': $(cat "$work/job-name")" ;;
esac
grep -q 'matrix: ${{ steps.select.outputs.matrix }}' "$workflow" || fail 'the groups job does not output the matrix of the select step'
grep -q 'jobs: ${{ steps.select.outputs.jobs }}' "$workflow" || fail 'the groups job does not output the jobs of the select step'
grep -q 'include: ${{ fromJSON(needs.groups.outputs.matrix) }}' "$workflow" || fail 'the gate job does not take its matrix from the groups job'
grep -q 'GATE_JOBS: ${{ needs.groups.outputs.jobs }}' "$workflow" || fail 'the final job is not given the jobs the matrix script names'
grep -q 'TOOLCHAIN_READY: sh .github/host-toolchain.sh wait' "$workflow" || fail 'the stage is not given the toolchain wait'
grep -q 'sh .github/host-toolchain.sh start' "$workflow" || fail 'the toolchain install is not started'
! grep -qE 'matrix\.stage|GATE_GROUPS|GATE_PLATFORMS' "$workflow" "$here/gate-both-platforms.sh" || fail 'the workflow or the final job script still reads the group x platform inputs'

# The two tests that the corpus parts name in full exist under those names in
# the conformance adapter, and the walk reads the variable the helper sets:
# a rename fails here and not in the first gate run that lists the tests.
for variable in walk receipts; do
    name=$(sed -n "s/^my \\\$$variable = '\\(.*\\)';\$/\\1/p" "$here/shard-tests.pl")
    [ -n "$name" ] || fail "shard-tests.pl names no $variable test"
    function=${name##*::}
    [ "${name%::*}" = conformance::adapter ] || fail "shard-tests.pl's $variable test is not in conformance::adapter: $name"
    [ "$(grep -c "^fn $function()" "$root/compiler/tests/conformance/adapter.rs")" = 1 ] || fail "the adapter has no single test $function"
    grep -B1 "^fn $function()" "$root/compiler/tests/conformance/adapter.rs" | grep -qx '#\[test\]' || fail "$function is not a test"
done
grep -qF 'std::env::var("WHITEFOOT_CONFORMANCE_SHARD")' "$root/compiler/tests/conformance/adapter.rs" || fail 'the adapter does not read WHITEFOOT_CONFORMANCE_SHARD'
grep -qF 'ENV{WHITEFOOT_CONFORMANCE_SHARD} = $walk_shard' "$here/shard-tests.pl" || fail 'shard-tests.pl does not set WHITEFOOT_CONFORMANCE_SHARD'

echo 'test-gate-matrix: ok'
