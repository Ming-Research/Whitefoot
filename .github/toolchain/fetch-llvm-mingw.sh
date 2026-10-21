#!/usr/bin/env bash
# No native tool fetches the llvm-mingw release this repository pins. Download
# it from its publisher, check the digest the pins file records, and unpack it,
# so the gate's Windows job and the release's Windows package start from the
# same bytes (design/compiler/toolchain-resolution).
# Usage: fetch-llvm-mingw.sh PINS_JSON ARCHIVE_DIRECTORY UNPACK_DIRECTORY
# Runs on a Windows host with Git's bash, 7z, curl, jq and sha256sum. An archive
# already in ARCHIVE_DIRECTORY is reused once its digest checks. The last line
# printed is the unpacked root, the directory that holds bin/clang.exe.
set -euo pipefail
if [ "$#" -ne 3 ]; then
  echo 'usage: fetch-llvm-mingw.sh PINS_JSON ARCHIVE_DIRECTORY UNPACK_DIRECTORY' >&2
  exit 2
fi
pins="$1"
archives=$(mkdir -p "$2" && cygpath -m "$2")
unpacked=$(mkdir -p "$3" && cygpath -m "$3")
source=$(jq -r .windows.source "$pins")
tag=$(jq -r .windows.tag "$pins")
asset=$(jq -r .windows.asset "$pins")
sha256=$(jq -r .windows.sha256 "$pins")
archive="$archives/$asset"
if [ ! -f "$archive" ]; then
  curl --fail --location --retry 3 --max-time 600 \
    "$source/releases/download/$tag/$asset" -o "$archive.part" >&2
  mv "$archive.part" "$archive"
fi
# Check cached bytes too. The digest is the publisher's, recorded in the pins;
# this does not authenticate a signature.
echo "$sha256  $archive" | sha256sum -c - >&2
rm -rf "$unpacked"
mkdir "$unpacked"
7z x -y -bd -o"$(cygpath -w "$unpacked")" "$(cygpath -w "$archive")" > /dev/null
root="$unpacked/${asset%.zip}"
test -x "$root/bin/clang.exe"
echo "$root"
