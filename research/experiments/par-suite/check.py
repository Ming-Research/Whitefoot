#!/usr/bin/env python3
"""CI-only instrument controls, using synthetic tables and a tiny child process."""
import copy
import subprocess
import sys
import tempfile
from pathlib import Path

from generate import grid
from run import require_kernel_verdict
from summarize import summarize


def main():
    cells = {"x": {"split": "visible", "expected_exit": "0"}}
    rows = [dict(cell="x", build=build, workers="2", round=str(round_number),
                 wall_ns="1000000", cpu_ns="900000", exit_status="0", checksum="123")
            for round_number in range(3) for build in ("seq", "par", "twin")]
    def reduce(data):
        return summarize(data, cells, ["2"], 3, 0.02, 0)[0]
    assert reduce(rows)["verdict"] == "pass"
    slow = copy.deepcopy(rows)
    for row in slow:
        if row["build"] != "seq":
            row["wall_ns"] = "1100000"
    assert reduce(slow)["verdict"] == "fail"
    noisy = copy.deepcopy(rows)
    for row in noisy:
        if row["build"] == "twin":
            row["wall_ns"] = "1100000"
    assert reduce(noisy)["verdict"] == "inconclusive"
    boundary = copy.deepcopy(rows)
    for row in boundary:
        if row["build"] != "seq":
            row["wall_ns"] = "1020000"
    assert reduce(boundary)["verdict"] == "pass"
    for key, value in (("checksum", "456"), ("exit_status", "7"), ("wall_ns", "0"),
                       ("cpu_ns", "-1"), ("workers", "4")):
        broken = copy.deepcopy(rows)
        broken[0][key] = value
        try:
            reduce(broken)
        except ValueError:
            pass
        else:
            raise AssertionError(f"bad {key} accepted")
    extra = copy.deepcopy(rows)
    extra[0][None] = ["surplus"]
    missing = copy.deepcopy(rows)
    missing[0]["checksum"] = None
    malformed = copy.deepcopy(rows)
    for row in malformed:
        row["checksum"] = "12a"
    for broken in (extra, missing, malformed):
        try:
            reduce(broken)
        except ValueError:
            pass
        else:
            raise AssertionError("a malformed raw row was accepted")
    program = copy.deepcopy(rows)
    for row in program:
        row["checksum"] = "exit:0;stdout-sha256:" + "0" * 64
    assert reduce(program)["verdict"] == "pass"
    for broken in (rows[:-1], rows + [rows[0]]):
        try:
            reduce(broken)
        except ValueError:
            pass
        else:
            raise AssertionError("missing/duplicate sample accepted")
    points = grid()
    assert len(points) == 216
    assert sum(c["split"] == "held-out" for c in points) == 108
    assert len({c["cell"] for c in points}) == 216
    assert points == grid()
    require_kernel_verdict(0, "VERDICT: PASS -- 0 kernel(s) adverse at two widths\n")
    require_kernel_verdict(1, "VERDICT: FAIL -- 1 kernel(s) adverse at two widths\n")
    for status, text in ((2, "REFUSED: missing mandelbrot W=2\n"), (0, ""),
                         (1, "REFUSED: invalid row\n"),
                         (0, "VERDICT: FAIL -- 1 kernel(s) adverse at two widths\n")):
        try:
            require_kernel_verdict(status, text)
        except RuntimeError:
            pass
        else:
            raise AssertionError("refused or contradictory kernel evidence accepted")
    # Exercise real exit propagation and process-group deadline detection. This
    # command is wired only into the explicitly dispatched CI experiment.
    with tempfile.TemporaryDirectory() as tmp:
        metrics = Path(tmp) / "metrics.tsv"
        for script, status, output in (("printf 123; exit 7", 7, b"123"), ("sleep 5", 124, b"")):
            result = subprocess.run([sys.argv[1], str(metrics), "1", "/bin/sh", "-c", script],
                                    check=True, capture_output=True)
            wall, cpu, actual = map(int, metrics.read_text().split())
            assert actual == status and result.stdout == output and wall > 0 and cpu >= 0
    print("par-suite instrument controls PASS")


if __name__ == "__main__":
    main()
