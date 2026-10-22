#!/bin/sh
# Choose which compiler releases the weekly cleanup removes
# (compiler-release-cleanup.yml, design/compiler/downstream-releases.md).
#
#   compiler-release-retention.sh CUTOFF < releases.tsv
#
# Each input line is `<published_at><TAB><tag>` for a `wf-*` release, newest
# first; CUTOFF is a timestamp in the same format. Each output line is
# `keep<TAB><tag><TAB><why>` or `remove<TAB><tag>`:
#
#   - a bundled release of a main commit, `wf-<12 hex>-bundled`, is an external
#     release and is never removed by age; it does not take the "newest
#     release of main" place either, which stays with the newest lightweight
#     release downstreams pin;
#   - the newest other release of a main commit is kept at any age;
#   - every other release published before CUTOFF is removed, experiment
#     releases (`wf-exp-*`, bundled or not) included.
set -eu
if [ "$#" -ne 1 ]; then
    echo 'usage: compiler-release-retention.sh CUTOFF < releases.tsv' >&2
    exit 2
fi
awk -F '\t' -v cutoff="$1" '
    $2 ~ /^wf-[0-9a-f]+-bundled$/ { printf "keep\t%s\tbundled\n", $2; next }
    $2 !~ /^wf-exp-/ && newest == "" { newest = $2; printf "keep\t%s\tnewest\n", $2; next }
    $1 < cutoff { printf "remove\t%s\n", $2 }
'
