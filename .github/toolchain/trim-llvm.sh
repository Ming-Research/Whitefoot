#!/usr/bin/env bash
# No native tool trims an LLVM release into a relocatable clang/lld subset.
# Use the platform's dependency inspectors and Python's path/JSON support to
# retain the original layout, discover its closure, and inventory the result.
# Usage: trim-llvm.sh EXTRACTED_LLVM EMPTY_OUTPUT SOURCE_ASSET SOURCE_SHA256 LICENSE_DIRECTORY
# This trial rejects fixed paths instead of rewriting binaries/install names.
set -euo pipefail
if [[ $# != 5 ]]; then
  echo "usage: $0 EXTRACTED_LLVM EMPTY_OUTPUT SOURCE_ASSET SOURCE_SHA256 LICENSE_DIRECTORY" >&2
  exit 2
fi
exec python3 - "$@" <<'PY'
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


def fail(message):
    raise SystemExit("trim-llvm: " + message)


source = Path(sys.argv[1]).resolve(strict=True)
output = Path(sys.argv[2]).resolve()
asset, source_sha256 = sys.argv[3:5]
license_directory = Path(sys.argv[5]).resolve(strict=True)
version = "22.1.8"
if not source.is_dir():
    fail("input must be an extracted LLVM release directory")
if not re.fullmatch(r"[0-9a-fA-F]{64}", source_sha256):
    fail("source SHA-256 must contain 64 hexadecimal digits")
if version not in asset:
    fail("source asset must identify LLVM " + version)
if source == output or source in output.parents or output in source.parents:
    fail("input and output directories must not overlap")
if output.exists() and (not output.is_dir() or any(output.iterdir())):
    fail("output must be absent or empty")
if sys.platform not in ("linux", "darwin"):
    fail("only Linux and macOS archives are supported by this trial")
for tool in (("readelf", "ldd") if sys.platform == "linux" else ("otool",)):
    if not shutil.which(tool):
        fail("required dependency inspector is missing: " + tool)
output.mkdir(parents=True, exist_ok=True)


def run(*args):
    environment = {key: value for key, value in os.environ.items()
                   if key not in ("LD_LIBRARY_PATH", "LD_PRELOAD") and not key.startswith("DYLD_")}
    result = subprocess.run(args, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE,
                            env={**environment, "LC_ALL": "C"})
    if result.returncode:
        fail("inspection failed: " + repr(args) + "\n" + result.stderr + result.stdout)
    return result.stdout


def inside(path):
    return path == source or source in path.parents


def relative(path):
    # Do not resolve here: the alias is a separately inventoried package path.
    return path.relative_to(source).as_posix()


kept = set()
copying = set()


def copy(path):
    path = Path(os.path.abspath(path))
    if not inside(path):
        fail("attempt to retain a path outside LLVM: " + str(path))
    rel = relative(path)
    if rel in kept:
        return
    if rel in copying:
        fail("symlink cycle: " + rel)
    copying.add(rel)
    destination = output / rel
    destination.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink():
        target = path.resolve(strict=True)
        if not inside(target):
            fail("symlink escapes the input: " + rel + " -> " + os.readlink(path))
        copy(target)
        # Normalize even absolute input symlinks to contained relative links.
        destination.symlink_to(os.path.relpath(output / relative(target), destination.parent))
    elif path.is_dir():
        destination.mkdir(exist_ok=True)
        for child in sorted(path.iterdir()):
            copy(child)
    elif path.is_file():
        shutil.copy2(path, destination)
    else:
        fail("missing or unsupported input member: " + str(path))
    copying.remove(rel)
    kept.add(rel)


for rel in ("bin/clang", "bin/lld", "lib/clang/22/include"):
    copy(source / rel)
if not (source / "lib/clang/22/include/stddef.h").is_file():
    fail("LLVM 22 builtin stddef.h is missing")
if not (source / "lib/clang/22/include/stdatomic.h").is_file():
    fail("LLVM 22 builtin stdatomic.h is missing")
# C runtime compilation needs builtin support and optional CRT start/end
# objects. Sanitizer, profiling, XRay and other optional runtime suites are
# outside this package's purpose. Preserve each allowed file's resource path.
runtime_pattern = re.compile(r"(?:lib)?clang_rt\.crt(?:begin|end)[^/]*\.o$|libclang_rt\.builtins[^/]*\.a$")
runtime_files = []


def retain_runtimes(directory, ancestors=()):
    real = directory.resolve(strict=True)
    if not inside(real) or real in ancestors:
        fail("escaping or cyclic runtime directory: " + str(directory))
    for path in sorted(directory.iterdir()):
        if path.is_dir():
            retain_runtimes(path, ancestors + (real,))
        elif (runtime_pattern.fullmatch(path.name) or
              (sys.platform == "darwin" and path.name == "libclang_rt.osx.a")):
            if not inside(path.resolve(strict=True)):
                fail("runtime escapes LLVM: " + str(path))
            copy(path)
            runtime_files.append(relative(path))


retain_runtimes(source / "lib/clang/22/lib")
if not any("builtins" in rel or rel.endswith("/libclang_rt.osx.a") for rel in runtime_files):
    fail("archive has no supported compiler-rt builtin library")
# Some distributions split the frontend or assembler out of the driver.
for name in ("clang-cc1", "clang-cc1as", "cc1", "cc1as"):
    if (source / "bin" / name).exists():
        copy(source / "bin" / name)
lld_aliases = ("ld.lld", "ld64.lld")
# If lld resolves to a driver alias, reversing that alias would create a cycle.
# Put the real bytes at bin/lld before installing the requested aliases.
lld_target = (source / "bin/lld").resolve(strict=True)
if relative(lld_target) in {"bin/" + alias for alias in lld_aliases}:
    (output / "bin/lld").unlink()
    shutil.copy2(lld_target, output / "bin/lld")
for alias in lld_aliases:
    destination = output / "bin" / alias
    if destination.exists() or destination.is_symlink():
        destination.unlink()
    destination.symlink_to("lld")
if sys.platform == "darwin":
    copy(source / "lib/libLTO.dylib")

# The archive may omit component licenses entirely. The caller supplies all
# four from the exact source release tag; validate them independently here.
license_name = re.compile(r"license|licence|copyright|notice|copying", re.I)
licenses = set()
license_sources = {}
for component in ("llvm", "clang", "lld", "compiler-rt"):
    path = license_directory / component / "LICENSE.TXT"
    if not path.is_file():
        fail("required component license is missing: " + str(path))
    content = path.read_text()
    if not content.strip() or "Apache License" not in content or "LLVM Exceptions" not in content:
        fail("required component license lacks Apache License and LLVM Exceptions: " + str(path))
    rel = "share/licenses/" + component + "/LICENSE.TXT"
    destination = output / rel
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, destination)
    licenses.add(rel)
    license_sources[rel] = "https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-22.1.8/" + component + "/LICENSE.TXT"


def copy_license(path, active=()):
    rel = path.relative_to(source)
    destination = output / "share/licenses" / rel
    if destination.exists() or destination.is_symlink():
        return
    if path in active:
        fail("license symlink cycle: " + str(rel))
    destination.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink():
        target = path.resolve(strict=True)
        if not inside(target):
            fail("license symlink escapes input: " + str(rel))
        copy_license(target, active + (path,))
        destination.symlink_to(os.path.relpath(output / "share/licenses" / target.relative_to(source), destination.parent))
    elif path.is_dir():
        destination.mkdir(exist_ok=True)
        for child in sorted(path.iterdir()):
            copy_license(child, active + (path,))
    elif path.is_file():
        shutil.copy2(path, destination)
    else:
        fail("unsupported license member: " + str(rel))
    if not path.is_dir() or path.is_symlink():
        licenses.add(destination.relative_to(output).as_posix())


def binary_kind(path):
    with path.open("rb") as stream:
        magic = stream.read(4)
    if magic == b"\x7fELF":
        return "elf"
    if magic in (b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe",
                 b"\xfe\xed\xfa\xcf", b"\xcf\xfa\xed\xfe",
                 b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca",
                 b"\xca\xfe\xba\xbf", b"\xbf\xba\xfe\xca"):
        return "macho"
    return None


host = {}
edges = []
inspected = set()
glibc_versions = set()
glibc_by_file = {}


def host_dependency(library, resolved, needed_by):
    # An LLVM installed on the CI host must never mask an incomplete closure.
    if re.search(r"/(?:llvm[^/]*|opt/homebrew|usr/local)/", resolved):
        fail("host/build LLVM dependency is not a system prerequisite: " + resolved)
    key = (library, resolved)
    host.setdefault(key, set()).add(needed_by)


def allowed_system(path):
    prefixes = (("/lib/", "/lib64/", "/usr/lib/", "/usr/lib64/")
                if sys.platform == "linux" else ("/usr/lib/", "/System/"))
    return str(path).startswith(prefixes)


def check_search_path(path, binary):
    # Relative loader tokens are relocatable; arbitrary absolute build paths
    # and host LLVM installations would make the package depend on the CI host.
    if path.startswith("/"):
        if allowed_system(path + "/") and not re.search(r"/llvm[^/]*/", path + "/"):
            return
        fail("nonrelocatable absolute library search path in " + binary + ": " + path)
    if sys.platform == "linux":
        if not (path.startswith("$ORIGIN") or path.startswith("${ORIGIN}")):
            fail("nonrelocatable ELF search path in " + binary + ": " + path)
    elif not path.startswith(("@loader_path", "@executable_path")):
        fail("unsupported/nonrelocatable LC_RPATH in " + binary + ": " + path)


def linux_dependencies(path, context=None):
    rel = relative(path)
    dynamic = run("readelf", "-d", str(path))
    needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", dynamic)
    for search in re.findall(r"\((?:RUNPATH|RPATH)\).*?\[(.*?)\]", dynamic):
        for component in search.split(":"):
            check_search_path(component, rel)
    versions = run("readelf", "--version-info", str(path))
    # Only requirements, not exported symbol-version definitions, set the floor.
    needs_section = versions.split("Version needs section", 1)
    if len(needs_section) == 2:
        required = set(re.findall(r"\bGLIBC_(\d+(?:\.\d+)+)\b", needs_section[1]))
        glibc_versions.update(required)
        if required:
            glibc_by_file[rel] = sorted(required, key=lambda value: tuple(map(int, value.split("."))))
    program_headers = run("readelf", "-l", str(path))
    interpreter = re.search(r"Requesting program interpreter: (.*?)\]", program_headers)
    if interpreter:
        loader = interpreter.group(1)
        if not allowed_system(loader) or not Path(loader).exists():
            fail("non-system/missing ELF interpreter in " + rel + ": " + loader)
        host_dependency(loader, str(Path(loader).resolve()), rel)
    if not needed:
        return []
    # Root ldd reports the full actual closure, including prerequisites reached
    # through system libraries. Carry that resolution into child inspection:
    # standalone ldd on a shared library can lose inherited executable RPATH.
    resolutions = context
    if resolutions is None:
        listing = run("ldd", str(path))
        if "not found" in listing:
            fail("unresolved ELF dependency in " + rel + "\n" + listing)
        resolutions = {}
        for line in listing.splitlines():
            match = re.match(r"\s*(\S+)\s+=>\s+(.+?)\s+\(0x[0-9a-fA-F]+\)", line)
            if match:
                resolutions[match.group(1)] = match.group(2)
            else:
                match = re.match(r"\s*(/.+?)\s+\(0x[0-9a-fA-F]+\)", line)
                if match:
                    # The program interpreter is listed by path only, but a
                    # binary may also name it in DT_NEEDED by its soname.
                    resolutions[match.group(1)] = match.group(1)
                    resolutions.setdefault(Path(match.group(1)).name, match.group(1))
    children = []
    for library in needed:
        if library.startswith("/") and not allowed_system(library):
            fail("nonrelocatable absolute DT_NEEDED in " + rel + ": " + library)
        if library not in resolutions:
            fail("ldd did not resolve " + library + " needed by " + rel)
    libraries = resolutions if context is None else needed
    for library in libraries:
        resolved = resolutions[library]
        dependency = Path(resolved)
        real = dependency.resolve(strict=True)
        if inside(real):
            if not inside(dependency):
                fail("in-tree dependency resolved via external alias: " + resolved)
            copy(dependency)
            edges.append({"needed_by": rel, "library": library,
                          "resolved_path": relative(dependency)})
            children.append((real, resolutions))
        elif allowed_system(dependency) and allowed_system(real):
            host_dependency(library, str(real), rel)
        else:
            fail("non-system dependency outside LLVM in " + rel + ": " + resolved)
    return children


def expand_darwin(reference, loader, executable):
    if reference.startswith("@loader_path"):
        return Path(str(loader.parent) + reference[len("@loader_path"):])
    if reference.startswith("@executable_path"):
        return Path(str(executable.parent) + reference[len("@executable_path"):])
    if reference.startswith("/"):
        return Path(reference)
    fail("unsupported Mach-O library reference: " + reference)


def darwin_dependencies(path, executable, inherited):
    rel = relative(path)
    loads = run("otool", "-l", str(path))
    rpaths = []
    for command in loads.split("Load command ")[1:]:
        if re.search(r"^\s*cmd LC_RPATH\s*$", command, re.M):
            match = re.search(r"^\s*path (.*?) \(offset \d+\)", command, re.M)
            if not match:
                fail("cannot parse LC_RPATH in " + rel)
            reference = match.group(1)
            check_search_path(reference, rel)
            rpaths.append(expand_darwin(reference, path, executable))
    search_paths = tuple(dict.fromkeys(rpaths + list(inherited)))
    dependencies = []
    for line in run("otool", "-L", str(path)).splitlines()[1:]:
        match = re.match(r"\s*(.*?) \(compatibility version ", line)
        if match:
            dependencies.append(match.group(1))
    # LC_ID_DYLIB is printed by -L as well; it is not a dependency edge.
    ids = re.findall(r"cmd LC_ID_DYLIB\s+cmdsize \d+\s+name (.*?) \(offset \d+\)", loads)
    dependencies = [item for item in dependencies if item not in ids]
    children = []
    for library in dependencies:
        if library.startswith("/"):
            if not allowed_system(library):
                fail("nonrelocatable absolute Mach-O dependency in " + rel + ": " + library)
            # macOS system libraries can exist only in the dyld shared cache.
            host_dependency(library, library, rel)
            continue
        if library.startswith("@rpath/"):
            candidates = [directory / library[len("@rpath/"):] for directory in search_paths]
        else:
            candidates = [expand_darwin(library, path, executable)]
        dependency = next((item for item in candidates if item.exists()), None)
        if dependency is None:
            fail("unresolved Mach-O dependency in " + rel + ": " + library +
                 "; searched " + repr([str(item) for item in candidates]))
        dependency = Path(os.path.abspath(dependency))
        real = dependency.resolve(strict=True)
        if inside(dependency) and inside(real):
            copy(dependency)
            edges.append({"needed_by": rel, "library": library,
                          "resolved_path": relative(dependency)})
            children.append((real, executable, search_paths))
        elif allowed_system(dependency) and allowed_system(real):
            host_dependency(library, str(real), rel)
        else:
            fail("Mach-O dependency escapes LLVM in " + rel + ": " + str(dependency))
    return children


# Inspect every retained binary, including CRT objects, and recursively inspect
# shared libraries required by the drivers and macOS libLTO.
queue = []
for rel in sorted(kept, reverse=True):
    path = source / rel
    if path.is_file():
        path = path.resolve(strict=True)
        kind = binary_kind(path)
        if kind:
            if kind != ("elf" if sys.platform == "linux" else "macho"):
                fail("archive binary format does not match this host: " + rel)
            executable = path if rel.startswith("bin/") else (source / "bin/clang").resolve()
            queue.append((path, executable, ()))
while queue:
    path, executable, inherited = queue.pop()
    key = (path, executable, inherited) if sys.platform == "darwin" else path
    if key in inspected:
        continue
    inspected.add(key)
    if sys.platform == "linux":
        queue.extend((child, child, context) for child, context in linux_dependencies(path, inherited or None))
    else:
        queue.extend(darwin_dependencies(path, executable, inherited))


# Retain archive notices attached to actual selected files or their immediate
# directories. Never search unrelated development trees, such as include/llvm,
# or directories belonging only to excluded optional resource runtimes.
generic_notice = re.compile(r"(?:licenses?|licences?|copyright|notices?|copying)(?:[._-].*)?$", re.I)
notice_candidates = set()
for rel in sorted(kept):
    path = source / rel
    if not path.is_file():
        continue
    if license_name.search(path.name):
        notice_candidates.add(path)
    for candidate in path.parent.iterdir():
        if generic_notice.fullmatch(candidate.name) or (
                license_name.search(candidate.name) and candidate.name.startswith(path.name + ".")):
            notice_candidates.add(candidate)
for path in sorted(notice_candidates):
    copy_license(path)


def digest(path):
    sha = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


files = []
for root, directories, names in os.walk(output, followlinks=False):
    # Include directory symlinks; os.walk otherwise puts them in directories.
    for name in sorted(names + [item for item in directories if (Path(root) / item).is_symlink()]):
        path = Path(root) / name
        entry = {"path": path.relative_to(output).as_posix()}
        if path.is_symlink():
            target = os.readlink(path)
            if os.path.isabs(target) or not path.resolve(strict=True).is_relative_to(output):
                fail("output symlink escapes package: " + entry["path"])
            payload = os.fsencode(target)
            entry.update(type="symlink", link_target=target, size=len(payload),
                         sha256=hashlib.sha256(payload).hexdigest())
        else:
            entry.update(type="file", size=path.stat().st_size, sha256=digest(path))
        files.append(entry)
minimum_glibc = max(glibc_versions, key=lambda value: tuple(map(int, value.split("."))), default=None)
manifest = {
    "llvm_version": version,
    "source_asset": asset,
    "source_sha256": source_sha256.lower(),
    "platform": sys.platform,
    "inventory_excludes": ["toolchain-manifest.json"],
    "hash_semantics": "Regular files hash file bytes; symlinks hash the encoded relative link target, and size is its byte length. Directories are omitted.",
    "files": sorted(files, key=lambda entry: entry["path"]),
    "licenses": sorted(licenses),
    "license_sources": license_sources,
    "compiler_rt_allowlist": "libclang_rt.builtins*.a; clang_rt.crtbegin*.o and clang_rt.crtend*.o (including lib-prefixed names); libclang_rt.osx.a on macOS only",
    "compiler_rt_files": sorted(runtime_files),
    "bundled_dependencies": [dict(needed_by=binary, library=library, resolved_path=resolved)
                             for binary, library, resolved in sorted({(entry["needed_by"], entry["library"], entry["resolved_path"]) for entry in edges})],
    "dependency_scope": "Linux root ldd records the full actual dependency closure (including indirect host prerequisites); child readelf DT_NEEDED edges use the root's resolution context. macOS records direct otool edges through the recursively inspected closure.",
    "host_dependencies": [dict(library=library, resolved_path=resolved, needed_by=sorted(binaries))
                          for (library, resolved), binaries in sorted(host.items())],
    "minimum_glibc_version": minimum_glibc,
    "minimum_glibc_scope": "Highest GLIBC symbol version required by retained ELF executables, shared libraries and CRT objects. Static builtin archive members are not scanned. This is a symbol requirement, not proof of minimum OS support; clean-host qualification is outstanding.",
    "glibc_requirements_by_file": glibc_by_file,
    "glibc_version_requirements": sorted(glibc_versions, key=lambda value: tuple(map(int, value.split(".")))),
}
(output / "toolchain-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print("Retained", len(files), "files/links from", asset)
print("Minimum required GLIBC symbol version:", minimum_glibc or "not applicable/no GLIBC version needs")
for entry in manifest["host_dependencies"]:
    print("Host prerequisite:", entry["library"], "=>", entry["resolved_path"], "needed by", ", ".join(entry["needed_by"]))
PY
