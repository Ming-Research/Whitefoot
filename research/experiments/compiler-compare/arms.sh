#!/usr/bin/env bash
# Builds the two compilers of the comparison (README.md beside this file).
#
#   arms.sh CANDIDATE BASE OUT
#
# Run from a full clone. CANDIDATE and BASE name a branch of origin or a
# commit; an empty BASE means the candidate's merge base with origin/main.
# Each arm is a worktree of its own commit under OUT/arms, built whole (the
# compiler, its standard library and its runtime), and its compiler is
# OUT/arms/ARM/compiler/target/gate/whitefootc. The same commit twice is
# allowed: that is the comparison's null run. Writes OUT/manifest.txt with the
# resolved commits and the host.
set -euo pipefail

if [ $# -ne 3 ]; then
    echo "usage: arms.sh CANDIDATE BASE OUT" >&2
    exit 2
fi
candidate_name=$1
base_name=$2
out=$3

git fetch --no-tags origin '+refs/heads/*:refs/remotes/origin/*'
resolve() {
    git rev-parse --verify -q "origin/$1^{commit}" || git rev-parse --verify "$1^{commit}"
}
candidate=$(resolve "$candidate_name")
if [ -n "$base_name" ]; then
    base=$(resolve "$base_name")
else
    base=$(git merge-base origin/main "$candidate")
fi

rm -rf "$out/arms"
git worktree prune
git worktree add --detach "$out/arms/base" "$base"
git worktree add --detach "$out/arms/candidate" "$candidate"

for arm in base candidate; do
    cargo fetch --locked --manifest-path "$out/arms/$arm/compiler/Cargo.toml"
    make -C "$out/arms/$arm/compiler" build
done

{
    echo "instrument $(git rev-parse HEAD)"
    echo "candidate $candidate"
    echo "base $base"
    if [ "$candidate" = "$base" ]; then
        echo "null run: the candidate and the base are the same commit"
    fi
    echo "clang $(/usr/bin/clang --version | head -1)"
    echo "cpu model $(lscpu | sed -n 's/^Model name: *//p')"
} > "$out/manifest.txt"
