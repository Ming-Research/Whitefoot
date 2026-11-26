#!/usr/bin/env bash
# The LLVM release archives may omit component licenses entirely. Download the
# license texts of llvm, clang, lld and compiler-rt from the exact source tag
# and check each one independently; the trim scripts copy them into the package.
# A bundle that also ships a runtime library of another LLVM project (the
# Windows bundle's libc++ and libunwind) names its extra components after the
# directory.
# Usage: fetch-licenses.sh LLVM_TAG DIRECTORY [EXTRA_COMPONENT...]
set -euo pipefail
if [ "$#" -lt 2 ]; then
  echo 'usage: fetch-licenses.sh LLVM_TAG DIRECTORY [EXTRA_COMPONENT...]' >&2
  exit 2
fi
tag="$1"
directory="$2"
shift 2
for component in llvm clang lld compiler-rt "$@"; do
  mkdir -p "$directory/$component"
  curl --fail --location --retry 3 --max-time 120 \
    "https://raw.githubusercontent.com/llvm/llvm-project/$tag/$component/LICENSE.TXT" \
    -o "$directory/$component/LICENSE.TXT"
  path="$directory/$component/LICENSE.TXT"
  if [ ! -s "$path" ] || ! grep -q 'Apache License' "$path" || ! grep -q 'LLVM Exceptions' "$path"; then
    echo "Missing Apache License or LLVM Exceptions in $path" >&2
    exit 1
  fi
done
