#!/bin/sh
# The jobs of one gate run, for the platforms it was asked for: the one place
# that decides which host runs a group in parts and which runs it whole.
#
#   sh .github/gate-matrix.sh '["ubuntu-24.04","macos-15"]'
#
# prints two lines for gate.yml to append to $GITHUB_OUTPUT:
#   matrix=<JSON array of {group, shard, name, os}, one object a job>
#   jobs=<JSON array of "<name> (<os>)", the jobs the final job requires>
#
# The groups and their parts come from the root Makefile's `check-groups`,
# which names no host. The Linux host runs every part the inventory lists; any
# other host runs each group once, whole (`all`), under the group's name. The
# job name is what gate-both-platforms.sh looks for and what compiler-release.yml
# matches by its `(macos-15)` ending. .github/test-gate-matrix.sh exercises it.
set -eu

split_hosts='["ubuntu-24.04"]'
platforms=${1:?usage: gate-matrix.sh PLATFORMS_JSON}
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
inventory=$(make -C "$root" --no-print-directory --silent check-groups)

matrix=$(jq -cn --argjson platforms "$platforms" --argjson inventory "$inventory" \
    --argjson split "$split_hosts" '
    def whole: reduce .[] as $part ([];
        if any(.[]; .group == $part.group) then .
        else . + [{group: $part.group, shard: "all", name: $part.group}] end);
    [$platforms[] as $os
        | ($inventory | if $split | index($os) then . else whole end)[]
        | . + {os: $os}]')
test "$(printf '%s' "$matrix" | jq 'length')" -gt 0
echo "matrix=$matrix"
echo "jobs=$(printf '%s' "$matrix" | jq -c 'map("\(.name) (\(.os))")')"
