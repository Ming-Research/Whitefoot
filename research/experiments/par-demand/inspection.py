"""Validate manually authored inspection against the selected build artifacts.

Native artifact tools do not bind a prose inspection to the measured images
and compiler IR. This experiment-only validator checks that binding; it never
infers that hot work survived or creates an inspection verdict.
"""
import hashlib
import json
import re
import argparse
from pathlib import Path

IR_ARMS = frozenset(('seq', 'par', 'demand', 'previous', 'unversioned'))
SHA256 = re.compile(r'[0-9a-f]{64}')
REVISION = re.compile(r'[0-9a-f]{40}')
# Citations use paths relative to the build, optionally followed by :line.
CITATION = re.compile(r'(?<![\w./-])(/?[\w.$+-]+(?:/[\w.$+-]+)*\.(?:disassembly|ledger))(?![\w.])')


def relative_artifact(build, filename):
    if not isinstance(filename, str) or not filename:
        raise ValueError('inspection artifact path missing')
    path = Path(filename)
    if path.is_absolute() or '..' in path.parts or path.as_posix() != filename:
        raise ValueError(f'invalid inspection artifact path: {filename}')
    artifact = build / path
    if not artifact.is_file() or not artifact.resolve().is_relative_to(build.resolve()):
        raise ValueError(f'inspection artifact missing or outside build: {filename}')
    return artifact


def validate_inspection(build, records, revision, workloads, arms):
    """Require a flat per-workload record and verify every supplied hash."""
    build = Path(build)
    if isinstance(records, (str, Path)):
        try:
            records = json.loads(Path(records).read_text())
        except (OSError, ValueError) as error:
            raise ValueError(f'inspection input missing or malformed: {records}') from error
    if not isinstance(revision, str) or not REVISION.fullmatch(revision):
        raise ValueError('inspection requires the measured build commit revision')
    if not isinstance(records, dict) or set(records) != set(workloads):
        raise ValueError('inspection workload records missing or unexpected')
    arms = tuple(arms)
    for name, record in records.items():
        if not isinstance(record, dict):
            raise ValueError(f'inspection record malformed: {name}')
        if record.get('revision') != revision:
            raise ValueError(f'inspection revision stale or missing: {name}')
        if type(record.get('hot_work_survives')) is not bool:
            raise ValueError(f'inspection hot_work_survives must be a boolean: {name}')
        for field in ('check_compiles_to', 'evidence'):
            if not isinstance(record.get(field), str) or not record[field].strip():
                raise ValueError(f'inspection {field} missing: {name}')
        hashes = record.get('hashes')
        if not isinstance(hashes, dict):
            raise ValueError(f'inspection artifact hashes missing: {name}')
        required = {f'{arm}/{name}' for arm in arms}
        required.update(f'{arm}/{name}.ll' for arm in arms if arm in IR_ARMS)
        for field in ('check_compiles_to', 'evidence'):
            required.update(CITATION.findall(record[field]))
        if not required <= set(hashes):
            raise ValueError(f'inspection required hashes missing: {name}: {sorted(required - set(hashes))}')
        for filename, digest in hashes.items():
            if not isinstance(digest, str) or not SHA256.fullmatch(digest):
                raise ValueError(f'inspection SHA256 malformed: {name}: {filename}')
            artifact = relative_artifact(build, filename)
            if hashlib.sha256(artifact.read_bytes()).hexdigest() != digest:
                raise ValueError(f'inspection artifact changed: {name}: {filename}')
    return records


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', required=True, type=Path)
    parser.add_argument('--experiment', required=True, choices=('1', '2', '3', '4', '5a', '5c', '6', '6b'))
    parser.add_argument('--revision', required=True)
    parser.add_argument('--inspection', type=Path)
    args = parser.parse_args()
    # Lazy import keeps the validator usable by the reducer without a cycle.
    from summarize import experiment_matrix, parse_experiment
    arms, _, workloads = experiment_matrix(parse_experiment(args.experiment))
    validate_inspection(args.build, args.inspection or args.build / 'inspection.json',
                        args.revision, workloads, arms)
    print(f'inspection binding PASS: experiment={args.experiment} revision={args.revision}')


if __name__ == '__main__':
    main()
