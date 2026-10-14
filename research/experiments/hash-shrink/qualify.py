#!/usr/bin/env python3
"""CI-only before/after qualification using the maintained fixture unchanged.

Only the final entry is selected; this script defines no independent copy of
the tests or library and changes no conformance result. Called by the temporary
manual workflow after its small library pilot.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

HERE = Path(__file__).resolve().parent
CASES = ("hash_map_test_shrink_oscillation", "hash_map_test_shrink_empty_reuse",
         "hash_map_test_shrink_regrow", "hash_map_test_growth",
         "hash_map_test_owned_shrink", "hash_map_test_shrink_million")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-root", type=Path, required=True)
    parser.add_argument("--head-root", type=Path, required=True)
    parser.add_argument("--base-compiler", type=Path, required=True)
    parser.add_argument("--head-compiler", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get("GITHUB_ACTIONS") != "true":
        parser.error("qualification runs in GitHub Actions only")
    fixture = args.head_root / "tests/programs/containers/hash-map-program.wf"
    original = fixture.read_text()
    marker = "\nfn main() -> status: ExitStatus pure {"
    if original.count(marker) != 1:
        raise RuntimeError("expected exactly one final maintained fixture entry")
    prefix, old_main = original.split(marker)
    if old_main.count("\nfn ") or not original.rstrip().endswith("}"):
        raise RuntimeError("maintained fixture main is not the final definition")
    generated = args.output / "generated"
    logs = args.output / "logs"
    generated.mkdir(parents=True)
    logs.mkdir()
    identities = {"fixture": str(fixture.relative_to(args.head_root)),
                  "fixture_sha256": hashlib.sha256(original.encode()).hexdigest(),
                  "selection": "replace final main only; each existing function returns its assertion failure count",
                  "sources": {}}
    for case in CASES:
        if "fn " + case + "() -> result: u64 pure {" not in prefix:
            raise RuntimeError("missing selected maintained test: " + case)
        selected = prefix + marker + "\n  let failures = " + case + "();\n  if failures != 0_u64 { return exit_status(code: 1_u8); }\n  return exit_status(code: 0_u8);\n}\n"
        (generated / (case + ".wf")).write_text(selected)
        identities["sources"][case] = hashlib.sha256(selected.encode()).hexdigest()
    (args.output / "identities.json").write_text(json.dumps(identities, indent=2) + "\n")
    # Time the smallest selected execution before admitting the million case.
    rows = []
    for index, case in enumerate(CASES):
        for arm in ("base", "head"):
            root = getattr(args, arm + "_root")
            image_dir = args.output / "images" / arm
            with (logs / (case + "-" + arm + "-build.log")).open("w") as stream:
                subprocess.run(["make", "-f", str(HERE / "Makefile"), "images", f"SHAPES={case}",
                                f"ROOT={root}", f"BUILD={image_dir}", f"SOURCES={generated}",
                                f"WFC={getattr(args, arm + '_compiler')}"], cwd=HERE,
                               stdout=stream, stderr=subprocess.STDOUT, check=True)
            command = [str(image_dir / case)]
            started = time.monotonic()
            result = subprocess.run(command, capture_output=True, text=True, timeout=300)
            elapsed = time.monotonic() - started
            expected = 1 if arm == "base" else 0
            row = {"case": case, "arm": arm, "command": command,
                   "source_sha256": identities["sources"][case], "wall_seconds": elapsed,
                   "status": result.returncode, "expected_status": expected,
                   "stdout": result.stdout, "stderr": result.stderr,
                   "pass": result.returncode == expected and not result.stdout and not result.stderr}
            rows.append(row)
            (args.output / "raw.json").write_text(json.dumps(rows, indent=2) + "\n")
            if not row["pass"]:
                raise RuntimeError(f"{case}/{arm}: wanted assertion status {expected}, got {result.returncode}; compile failures/crashes are not a before-failure")
            if index == 0 and elapsed > 30:
                raise RuntimeError("small qualification pilot exceeded 30 seconds; inspect before scaling")
    print("Six selected maintained conditions: baseline assertion failure, candidate success; raw statuses recorded.")


if __name__ == "__main__":
    main()
