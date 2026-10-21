#!/usr/bin/env bash
# Builds the two compilers of the comparison (README.md beside this file).
#
#   arms.sh CANDIDATE BASE OUT
#
# Run from a full clone. CANDIDATE and BASE name a branch of origin or a
# commit; an empty BASE means the candidate's merge base with origin/main.
# Both arms are worktrees of the candidate commit under OUT/arms; the base
# arm's lib/std/collections/hash_map/ is replaced by the base commit's. Each
# arm's compiler is OUT/arms/ARM/compiler/target/gate/whitefootc. Exits 1
# when the two arms' hash-map directories are the same. Writes
# OUT/manifest.txt with the resolved commits and the host.
set -euo pipefail

if [ $# -ne 3 ]; then
    echo "usage: arms.sh CANDIDATE BASE OUT" >&2
    exit 2
fi
candidate_name=$1
base_name=$2
out=$3
library=lib/std/collections/hash_map

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
for arm in base candidate; do
    git worktree add --detach "$out/arms/$arm" "$candidate"
done
git -C "$out/arms/base" rm -rq "$library"
git -C "$out/arms/base" checkout "$base" -- "$library"
git -C "$out/arms/base" status --short
if git -C "$out/arms/base" diff --quiet HEAD; then
    echo "the base and the candidate have the same $library/" >&2
    exit 1
fi

for arm in base candidate; do
    cargo fetch --locked --manifest-path "$out/arms/$arm/compiler/Cargo.toml"
    make -C "$out/arms/$arm/compiler" build
done

{
    echo "instrument $(git rev-parse HEAD)"
    echo "candidate $candidate"
    echo "base $base"
    echo "clang $(/usr/bin/clang --version | head -1)"
    echo "cpu model $(lscpu | sed -n 's/^Model name: *//p')"
} > "$out/manifest.txt"
