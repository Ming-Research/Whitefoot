#!/usr/bin/env python3
"""Write SHA256SUMS at a package root, covering every regular file.

Native SHA256SUMS covers regular-file bytes, including the compiler, language
inputs and toolchain manifest. The manifest separately inventories relative
symlink text; do not hash symlink referents twice.

Usage: write-checksums.py PACKAGE_ROOT
"""
import hashlib
import os
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
lines = []
for directory, _, names in os.walk(root, followlinks=False):
    for name in names:
        path = pathlib.Path(directory) / name
        if path.is_symlink() or path == root / "SHA256SUMS":
            continue
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(block)
        lines.append(f"{digest.hexdigest()}  {path.relative_to(root).as_posix()}\n")
(root / "SHA256SUMS").write_text("".join(sorted(lines, key=lambda line: line[66:])))
