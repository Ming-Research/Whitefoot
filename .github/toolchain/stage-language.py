#!/usr/bin/env python3
"""Copy the specification and the standard library interface files of the
checked-out commit into a package root, at their repository paths.

Usage: stage-language.py PACKAGE_ROOT   (run in the commit's checkout)
"""
import pathlib
import shutil
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
names = subprocess.check_output(["git", "ls-files", "-z", "spec/kernel-spec.md",
    "lib/std/modules.wfg", "lib/std/**/module.wfm"]).split(b"\0")
for name in filter(None, names):
    path = pathlib.Path(name.decode())
    destination = root / path
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, destination)
for required in ("spec/kernel-spec.md", "lib/std/modules.wfg"):
    if not (root / required).is_file():
        raise SystemExit("Missing bundled language input: " + required)
