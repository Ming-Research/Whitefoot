#!/usr/bin/env python3
"""Check what a cold and a warm cached build of one witness reported.

The report's final JSON line contains build observations; diagnostics are
retained in the same log without making a substring a cache-hit proof. A cold
build must compile objects and reuse none, and the warm build after it must
reuse objects and compile none.

Usage: check-cache-report.py LOGS_DIRECTORY KIND   (KIND is objects or fragments)
Reads LOGS_DIRECTORY/KIND-cold-build.txt and KIND-warm-build.txt.
"""
import json
import pathlib
import sys

logs, kind = pathlib.Path(sys.argv[1]), sys.argv[2]


def report(temperature):
    rows = (logs / f'{kind}-{temperature}-build.txt').read_text().splitlines()
    return next(json.loads(row)['build'] for row in reversed(rows) if row.startswith('{"build":'))


cold, warm = report('cold'), report('warm')
assert cold['objects_compiled'] > 0 and cold['objects_reused'] == 0, cold
assert warm['objects_reused'] > 0 and warm['objects_compiled'] == 0, warm
if kind == 'fragments':
    assert cold['fragments'] > 0 and warm['fragments'] > 0, (cold, warm)
print(kind, 'cold/warm cache observations:', cold, warm)
