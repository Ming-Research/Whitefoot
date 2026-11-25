#!/usr/bin/env python3
"""Checks that a package's compiler takes the clang answers its manifest records,
and probes the clang again, saying why, when it cannot.

Usage:
  clang-facts-checks.py report LOG manifest
      The last `--report` line of LOG says the answers came from the manifest,
      that no probe ran, and the log carries no fallback note.
  clang-facts-checks.py report LOG probed TEXT...
      It says the clang was probed (at least three probe commands), that the
      reason names every TEXT, and that the log carries the fallback note with
      that reason.
  clang-facts-checks.py recorded MANIFEST CLANG_VERSION_LINE
      The manifest's clang_facts holds the version line of the package's clang
      and one well-formed answer for each probe (the macOS stack probe on
      macOS only, named by CLANG_FACTS_PLATFORM=macos).
  clang-facts-checks.py edit MANIFEST version|answer
      Make the record stale: another clang version, or one answer deleted.
"""
import json
import os
import pathlib
import sys


def fail(message):
    raise SystemExit("clang-facts-checks: " + message)


def last_report(log):
    rows = [row.strip() for row in log.splitlines()]
    return next((json.loads(row) for row in reversed(rows) if row.startswith('{"build":')), None)


command = sys.argv[1]
if command == "report":
    path, expected, texts = pathlib.Path(sys.argv[2]), sys.argv[3], sys.argv[4:]
    log = path.read_text()
    report = last_report(log)
    if report is None or not isinstance(report.get("toolchain_facts"), dict):
        fail("the build printed no toolchain_facts in its --report line: " + str(path))
    facts = report["toolchain_facts"]
    if facts["source"] != expected:
        fail("the answers came from %r, not %r: %s" % (facts["source"], expected, json.dumps(facts)))
    if expected == "manifest":
        if facts["probes_run"] != 0 or facts["reason"] is not None:
            fail("answers taken from the manifest, yet: " + json.dumps(facts))
        if "whitefootc: note:" in log:
            fail("a note about probing was printed though no probe ran")
    else:
        if facts["probes_run"] < 3 or not facts["reason"]:
            fail("the clang was probed without a reason or probes: " + json.dumps(facts))
        for text in texts:
            if text not in facts["reason"]:
                fail("the reason %r does not name %r" % (facts["reason"], text))
        if "whitefootc: note:" not in log or facts["reason"] not in log.replace("\r", ""):
            fail("the diagnostic does not say why the clang was probed")
    print("report ok:", json.dumps(facts))
elif command == "recorded":
    manifest = json.loads(pathlib.Path(sys.argv[2]).read_text())
    facts = manifest.get("clang_facts")
    if not isinstance(facts, dict):
        fail("the manifest records no clang_facts")
    if facts.get("clang_version") != sys.argv[3]:
        fail("recorded clang_version %r is not the clang's %r" % (facts.get("clang_version"), sys.argv[3]))
    if not isinstance(facts.get("preserve_none"), bool):
        fail("preserve_none is not recorded as true or false")
    if facts.get("no_capture_attribute") not in ("captures(none)", "nocapture"):
        fail("no_capture_attribute is not recorded")
    if facts.get("coro_end_result") not in ("void", "i1"):
        fail("coro_end_result is not recorded")
    if os.environ.get("CLANG_FACTS_PLATFORM") == "macos" and not isinstance(
            facts.get("darwin_stack_probe_helper"), bool):
        fail("the macOS package records no stack-probe answer")
    print("recorded ok:", json.dumps(facts))
elif command == "edit":
    path, how = pathlib.Path(sys.argv[2]), sys.argv[3]
    manifest = json.loads(path.read_text())
    if how == "version":
        manifest["clang_facts"]["clang_version"] = "clang version 0.0.0 (edited by the package check)"
    elif how == "answer":
        del manifest["clang_facts"]["preserve_none"]
    else:
        fail("unknown edit " + how)
    path.write_text(json.dumps(manifest, indent=2) + "\n")
else:
    fail("unknown command " + command)
