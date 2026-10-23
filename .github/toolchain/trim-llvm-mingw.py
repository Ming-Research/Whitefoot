#!/usr/bin/env python3
"""Trim an unpacked llvm-mingw release into the x86-64 clang/lld subset a
Windows Whitefoot package carries, and inventory the result.

No native tool trims llvm-mingw into a relocatable subset. Keep its layout so
the bundled clang finds its headers, import libraries and compiler-rt builtins
relative to its own directory, discover the DLL closure of the retained tools
with the release's own llvm-readobj, and reject any DLL that is neither bundled
nor part of Windows itself.

Usage: trim-llvm-mingw.py EXTRACTED_LLVM_MINGW EMPTY_OUTPUT SOURCE_ASSET \
           SOURCE_SHA256 LICENSE_DIRECTORY
Runs on Windows, where the retained tools can be executed.
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

VERSION = "22.1.8"
TRIPLE = "x86_64-w64-windows-gnu"
SYSROOT = "x86_64-w64-mingw32"
RESOURCE = "lib/clang/22"
# The bundled DLLs that were reviewed for their licenses: LLVM's own libraries
# are covered by the license text of their LLVM project (llvm, clang, libcxx
# and libunwind) that the package carries, winpthreads by the notice that
# travels in the sysroot. A new DLL in the closure fails the trim until someone
# has read its license.
REVIEWED_BUNDLED_DLLS = {"libllvm-22.dll", "libclang-cpp.dll", "libc++.dll",
                         "libunwind.dll", "libwinpthread-1.dll"}
# Runtime libraries a clean Windows machine does not have: a dependency on one
# means the package would not run there.
REDISTRIBUTABLE = re.compile(r"(vcruntime|msvcp|concrt|vccorlib|vcomp|ucrtbased|libgcc|libstdc\+\+)", re.I)


def fail(message):
    raise SystemExit("trim-llvm-mingw: " + message)


if len(sys.argv) != 6:
    fail("usage: EXTRACTED_LLVM_MINGW EMPTY_OUTPUT SOURCE_ASSET SOURCE_SHA256 LICENSE_DIRECTORY")
if sys.platform != "win32":
    fail("the retained tools are Windows executables; run this on Windows")
source = Path(sys.argv[1]).resolve(strict=True)
output = Path(sys.argv[2]).resolve()
asset, source_sha256 = sys.argv[3:5]
license_directory = Path(sys.argv[5]).resolve(strict=True)
if not re.fullmatch(r"[0-9a-fA-F]{64}", source_sha256):
    fail("source SHA-256 must contain 64 hexadecimal digits")
if source == output or source in output.parents or output in source.parents:
    fail("input and output directories must not overlap")
if output.exists() and (not output.is_dir() or any(output.iterdir())):
    fail("output must be absent or empty")
output.mkdir(parents=True, exist_ok=True)
bin_directory = source / "bin"
readobj = bin_directory / "llvm-readobj.exe"
for required in (bin_directory / "clang.exe", bin_directory / "ld.lld.exe", readobj,
                 source / SYSROOT / "lib", source / "include", source / RESOURCE / "include" / "stddef.h",
                 source / RESOURCE / "lib" / "windows" / "libclang_rt.builtins-x86_64.a",
                 source / "LICENSE.TXT"):
    if not required.exists():
        fail("the release lacks " + str(required.relative_to(source)))


def run(*arguments):
    result = subprocess.run([str(item) for item in arguments], stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True)
    if result.returncode:
        fail("command failed: " + repr(arguments) + "\n" + result.stderr + result.stdout)
    return result.stdout


version_line = run(bin_directory / "clang.exe", "--version").splitlines()[0]
if "clang version " + VERSION not in version_line:
    fail("expected LLVM " + VERSION + ", the release's clang says: " + version_line)

def relative(path):
    return path.relative_to(source).as_posix()


def copy(path, destination_relative=None):
    """Copy a file or a whole directory, keeping its layout (or placing it)."""
    rel = destination_relative or relative(path)
    destination = output / rel
    if path.is_dir():
        destination.mkdir(parents=True, exist_ok=True)
        for child in sorted(path.iterdir()):
            copy(child, rel + "/" + child.name)
    elif path.is_file():
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, destination)
    else:
        fail("missing or unsupported input member: " + str(path))


# clang.exe is a small launcher; it runs the real driver, clang-<major>.exe,
# which must come with it.
drivers = sorted(item.name for item in bin_directory.iterdir()
                 if re.fullmatch(r"clang-\d+\.exe", item.name))
if drivers != ["clang-22.exe"]:
    fail("expected exactly bin/clang-22.exe as the real clang driver, found " + repr(drivers))
tools = ["clang.exe", "clang-22.exe", "ld.lld.exe"]
for name in tools:
    copy(bin_directory / name)
# clang reads its target's configuration (runtime library, unwinder, default
# linker) from bin/<triple>.cfg; they are a few bytes each.
for name in ("x86_64-w64-windows-gnu.cfg", "x86_64-pc-windows-gnu.cfg", "mingw32-common.cfg"):
    copy(bin_directory / name)
# The x86-64 MinGW sysroot (import libraries, startup objects, the C runtime
# glue, license notices) and the headers shared by every MinGW target. libc++'s
# C++ headers are left out: Whitefoot compiles C and LLVM IR only.
copy(source / SYSROOT)
for child in sorted((source / "include").iterdir()):
    if child.name != "c++":
        copy(child)
# Clang's builtin headers, and of the compiler-rt runtime only the builtins
# archive the x86-64 Windows link needs (it supplies ___chkstk_ms).
copy(source / RESOURCE / "include")
runtime_files = [RESOURCE + "/lib/windows/libclang_rt.builtins-x86_64.a"]
copy(source / runtime_files[0])

# The DLL closure of the retained tools through bin, delay-loaded imports
# included. A name found in bin is bundled; any other must be Windows itself.
bundled_names = {item.name.lower(): item.name for item in bin_directory.iterdir()
                 if item.suffix.lower() == ".dll"}
system_directory = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32"
edges = []
host = {}
queue = [bin_directory / name for name in tools]
seen = set()
while queue:
    path = queue.pop()
    if path in seen:
        continue
    seen.add(path)
    listing = run(readobj, "--coff-imports", path)
    names = sorted(set(re.findall(r"^\s*Name:\s*(\S+\.dll)\s*$", listing, re.M | re.I)))
    for name in names:
        key = name.lower()
        if key in bundled_names:
            if key not in REVIEWED_BUNDLED_DLLS:
                fail("unreviewed bundled DLL " + bundled_names[key] + " needed by " + path.name +
                     "; read its license and add it to REVIEWED_BUNDLED_DLLS")
            dependency = bin_directory / bundled_names[key]
            copy(dependency)
            edges.append({"needed_by": relative(path), "library": name, "resolved_path": relative(dependency)})
            queue.append(dependency)
        elif REDISTRIBUTABLE.match(key):
            fail(path.name + " needs " + name + ", which a clean Windows machine does not have")
        elif key.startswith(("api-ms-win-", "ext-ms-win-")):
            # API set names resolve inside the loader; there is no file.
            host.setdefault((name, "api set"), set()).add(relative(path))
        elif (system_directory / name).is_file():
            host.setdefault((name, "System32"), set()).add(relative(path))
        else:
            fail(path.name + " needs " + name + ", which is neither bundled nor in " + str(system_directory))

# Licenses: the LLVM projects whose code the package carries, from the exact
# source tag (the caller downloads them), the release's own top-level license,
# and the MinGW-w64 notices that travel in the sysroot.
licenses = []
license_sources = {}
for component in ("llvm", "clang", "lld", "compiler-rt", "libcxx", "libunwind"):
    path = license_directory / component / "LICENSE.TXT"
    if not path.is_file():
        fail("required component license is missing: " + str(path))
    content = path.read_text()
    if not content.strip() or "Apache License" not in content or "LLVM Exceptions" not in content:
        fail("required component license lacks Apache License and LLVM Exceptions: " + str(path))
    rel = "share/licenses/" + component + "/LICENSE.TXT"
    (output / rel).parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, output / rel)
    licenses.append(rel)
    license_sources[rel] = "https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-" + VERSION + "/" + component + "/LICENSE.TXT"
rel = "share/licenses/llvm-mingw/LICENSE.TXT"
(output / rel).parent.mkdir(parents=True, exist_ok=True)
shutil.copy2(source / "LICENSE.TXT", output / rel)
licenses.append(rel)
license_sources[rel] = asset + ":LICENSE.TXT"
notices = sorted(item for item in (source / SYSROOT / "share" / "mingw32").iterdir() if item.name.startswith("COPYING"))
if not any(item.name == "COPYING.winpthreads.txt" for item in notices):
    fail("the sysroot carries no winpthreads notice")
for item in notices:
    licenses.append(relative(item))
    license_sources[relative(item)] = asset + ":" + relative(item)


def digest(path):
    sha = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


files = []
for root, _, names in os.walk(output):
    for name in sorted(names):
        path = Path(root) / name
        files.append({"path": path.relative_to(output).as_posix(), "type": "file",
                      "size": path.stat().st_size, "sha256": digest(path)})
manifest = {
    "llvm_version": VERSION,
    "source_asset": asset,
    "source_sha256": source_sha256.lower(),
    "platform": "windows-x86_64",
    "target_triple": TRIPLE,
    "inventory_excludes": ["toolchain-manifest.json"],
    "hash_semantics": "Regular files hash file bytes. Directories are omitted.",
    "files": sorted(files, key=lambda entry: entry["path"]),
    "licenses": sorted(licenses),
    "license_sources": license_sources,
    "compiler_rt_allowlist": "libclang_rt.builtins-x86_64.a",
    "compiler_rt_files": runtime_files,
    "bundled_dependencies": [dict(needed_by=binary, library=library, resolved_path=resolved)
                             for binary, library, resolved in sorted({(entry["needed_by"], entry["library"], entry["resolved_path"]) for entry in edges})],
    "dependency_scope": "llvm-readobj --coff-imports of the retained executables and of every bundled DLL they load, delay-loaded imports included.",
    "host_dependencies": [dict(library=library, resolved_path=resolved, needed_by=sorted(binaries))
                          for (library, resolved), binaries in sorted(host.items())],
    "windows_runtime_scope": "The retained tools and the programs they link import only Windows system DLLs, among them the Universal C Runtime that ships with Windows 10 and later. This lists dependencies; it is not proof of a minimum Windows version.",
}
(output / "toolchain-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print("Retained", len(files), "files from", asset)
print("Bundled DLLs:", ", ".join(sorted({entry["library"] for entry in edges})))
for entry in manifest["host_dependencies"]:
    print("System dependency:", entry["library"], "(" + entry["resolved_path"] + ") needed by", ", ".join(entry["needed_by"]))
