#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"
: "${WHITEFOOTC:?Set WHITEFOOTC to the compiler executable}"
: "${LUA:?Set LUA to the PUC Lua 5.1.5 executable}"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/halo-vm.XXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
"$LUA" research/experiments/halo-vm/oracle.lua > "$scratch/oracle.txt"
diff -u research/experiments/halo-vm/oracle.expected "$scratch/oracle.txt"
perl .github/run-check.pl halo-vm-modules "$WHITEFOOTC" --cache "$scratch/cache" --graph lib/halo/modules.wfg --check-modules
perl .github/run-check.pl halo-vm-smoke-build "$WHITEFOOTC" --cache "$scratch/cache" --graph research/experiments/halo-vm/modules.wfg --entry smoke -o "$scratch/smoke"
"$scratch/smoke"
printf '%s\n' 'PASS smoke'
perl .github/run-check.pl halo-vm-suite-build "$WHITEFOOTC" --cache "$scratch/cache" --graph research/experiments/halo-vm/modules.wfg --entry suite -o "$scratch/suite"
if "$scratch/suite"; then
  printf '%s\n' 'PASS fib numeric_for counter tables meta protected_error vararg tail budget_one budget_seven'
else
  status=$?
  printf 'FAIL suite: fixture index %s (see suite in test/cases.wf)\n' "$status"
  exit "$status"
fi
