#!/bin/sh
# Check .github/gate-matrix.sh against the real Makefile inventory and against a
# stand-in inventory: which host runs parts and which runs groups whole, the
# names the final job requires, and that every job name is one a file or
# artifact may carry, and that `make check-groups` is still the list of group
# names that main's gate.yml reads when the engine checks a commit with it. It
# runs `make` for the two inventories and jq, nothing heavier.
set -eu
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

echo 'test-gate-matrix: ok'
