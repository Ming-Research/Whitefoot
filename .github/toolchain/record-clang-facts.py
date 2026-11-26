#!/usr/bin/env python3
"""Record the answers of a package's clang in the package's manifest.

The compiler asks the clang it finds beside itself a few questions the first
time it generates native code (which spelling of an LLVM attribute the clang
takes, what an intrinsic returns, whether a calling convention works, and on
macOS which stack probe it lowers). A package carries one fixed clang, so the
release asks once and the manifest keeps the answers under `clang_facts`; the
compiler in the package then takes them instead of asking, for as long as the
first line of the clang's `--version` is the one recorded.

The answers come from the compiler's own `--report` of a build made with the
package's compiler while the manifest still holds none, so there is one set of
probes, the compiler's. The report must say the clang was probed and why the
manifest was not used: a manifest that already holds answers would hand back
its own, and nothing would have asked the clang.

Usage: record-clang-facts.py REPORT_OUTPUT MANIFEST
  REPORT_OUTPUT  the standard output of `whitefootc --report -o ... PROGRAM`
  MANIFEST       the package's toolchain-manifest.json, rewritten in place
"""
import json
import pathlib
import sys

REQUIRED = ("clang_version", "preserve_none", "no_capture_attribute", "coro_end_result")
OPTIONAL = ("darwin_stack_probe_helper",)


def fail(message):
    raise SystemExit("record-clang-facts: " + message)


report_path, manifest_path = map(pathlib.Path, sys.argv[1:3])
rows = [row.strip() for row in report_path.read_text().splitlines()]
build = next((json.loads(row) for row in reversed(rows) if row.startswith('{"build":')), None)
if build is None:
    fail("the build printed no --report line")
facts = build.get("toolchain_facts")
if not isinstance(facts, dict):
    fail("the compiler's report has no toolchain_facts; this compiler cannot record a package's answers")
if facts.get("source") != "probed" or "clang_facts" not in (facts.get("reason") or ""):
    fail("the clang was not probed because the manifest holds no clang_facts: " + json.dumps(facts))
if not facts.get("probes_run"):
    fail("the report says no probe ran: " + json.dumps(facts))
missing = [key for key in REQUIRED if facts.get(key) is None]
if missing:
    fail("the report lacks " + ", ".join(missing))

manifest = json.loads(manifest_path.read_text())
if "clang_facts" in manifest:
    fail("the manifest already holds clang_facts")
recorded = {key: facts[key] for key in REQUIRED + OPTIONAL if key in facts}
manifest["clang_facts"] = dict(sorted(recorded.items()))
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
print("Recorded the answers of", facts["clang_version"])
print(json.dumps(manifest["clang_facts"], indent=2))
