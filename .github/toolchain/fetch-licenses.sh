#!/usr/bin/env bash
# The LLVM release archives may omit component licenses entirely. Download the
# four components' license texts from the exact source tag and check each one
# independently; the trim scripts copy them into the package.
# Usage: fetch-licenses.sh LLVM_TAG DIRECTORY
set -euo pipefail
if [ "$#" -ne 2 ]; then
  echo 'usage: fetch-licenses.sh LLVM_TAG DIRECTORY' >&2
  exit 2
fi
tag="$1"
directory="$2"
for component in llvm clang lld compiler-rt; do
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
