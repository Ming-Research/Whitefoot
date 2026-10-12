"""Authored inspection-binding cases; no compiler or native image execution."""
import copy
import contextlib
import io
import hashlib
import json
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path
from inspection import IR_ARMS, validate_inspection
import inspection as inspector
from summarize import bound_inspection
import summarize as reducer


class InspectionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.build = Path(self.directory.name)
        self.revision = 'a' * 40
        self.names = ('fixture',)
        self.arms = ('seq', 'par', 'demand', 'previous', 'unversioned', 'twin',
                     'seq-shift64', 'seq-shift4160', 'seq-shift65664')
        hashes = {}
        files = [f'{arm}/fixture' for arm in self.arms]
        files += [f'{arm}/fixture.ll' for arm in self.arms if arm in IR_ARMS]
        files += ['demand/fixture.disassembly', 'demand/fixture.ll.ledger']
        for filename in files:
            path = self.build / filename
            path.parent.mkdir(exist_ok=True)
            path.write_bytes(filename.encode())
            hashes[filename] = hashlib.sha256(path.read_bytes()).hexdigest()
        self.records = {'fixture': dict(revision=self.revision, hashes=hashes,
            hot_work_survives=True, evidence='demand/fixture.disassembly:12; demand/fixture.ll.ledger',
            check_compiles_to='the inspected call and loop survive')}

    def check(self, records=None):
        return validate_inspection(self.build, self.records if records is None else records,
                                   self.revision, self.names, self.arms)

    def test_full_selected_images_ir_and_named_evidence_pass(self):
        self.assertEqual(self.check(), self.records)
        record = copy.deepcopy(self.records)
        record['fixture']['hot_work_survives'] = False
        self.assertFalse(self.check(record)['fixture']['hot_work_survives'])
        path = self.build / 'inspection.json'
        path.write_text(json.dumps(self.records))
        self.assertEqual(self.check(path), self.records)

    def test_missing_inputs_fields_images_ir_and_citations_fail(self):
        with self.assertRaisesRegex(ValueError, 'inspection input'):
            self.check(self.build / 'missing.json')
        with self.assertRaisesRegex(ValueError, 'workload'):
            self.check({})
        for field in ('revision', 'hashes', 'hot_work_survives', 'evidence', 'check_compiles_to'):
            changed = copy.deepcopy(self.records)
            del changed['fixture'][field]
            with self.subTest(field=field), self.assertRaises(ValueError): self.check(changed)
        for filename in self.records['fixture']['hashes']:
            changed = copy.deepcopy(self.records)
            del changed['fixture']['hashes'][filename]
            with self.subTest(filename=filename), self.assertRaisesRegex(ValueError, 'required hashes'):
                self.check(changed)

    def test_changed_ir_image_or_inspected_ledger_rejects_stale_record(self):
        for filename in ('previous/fixture.ll', 'twin/fixture', 'demand/fixture.ll.ledger'):
            path = self.build / filename
            original = path.read_bytes()
            path.write_bytes(original + b' changed')
            with self.subTest(filename=filename), self.assertRaisesRegex(ValueError, 'artifact changed'):
                self.check()
            path.write_bytes(original)

    def test_stale_revision_bad_types_and_unbound_paths_fail(self):
        for field, value in [('revision', 'b' * 40), ('hot_work_survives', 'true'),
                             ('evidence', '  '), ('check_compiles_to', '')]:
            changed = copy.deepcopy(self.records)
            changed['fixture'][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError): self.check(changed)
        for filename in ('../outside', '/tmp/outside'):
            changed = copy.deepcopy(self.records)
            changed['fixture']['hashes'][filename] = 'a' * 64
            with self.subTest(filename=filename), self.assertRaisesRegex(ValueError, 'artifact path'):
                self.check(changed)
        changed = copy.deepcopy(self.records)
        changed['fixture']['evidence'] += '; demand/other.disassembly'
        with self.assertRaisesRegex(ValueError, 'required hashes'): self.check(changed)

    def test_common_reducer_requires_measured_revision_and_inspection(self):
        path = self.build / 'measurements.tsv'
        with self.assertRaisesRegex(ValueError, 'identity.json'):
            bound_inspection(path, None, 6)
        (self.build / 'identity.json').write_text(json.dumps(dict(revision=self.revision)))
        # The common experiment consumer requires its complete manifest,
        # rather than accepting this otherwise valid one-workload fixture.
        with self.assertRaisesRegex(ValueError, 'workload'):
            bound_inspection(path, self.records, 6)

    def test_cli_missing_explicit_input_fails_before_measurement_loading(self):
        (self.build / 'identity.json').write_text(json.dumps(dict(revision=self.revision)))
        with patch('sys.argv', ['summarize.py', str(self.build / 'measurements.tsv'),
                               '--inspection', str(self.build / 'missing.json')]), \
                self.assertRaisesRegex(ValueError, 'inspection input'):
            reducer.main()

    def test_inspection_cli_selects_the_complete_matrix_and_default_input_path(self):
        with patch('sys.argv', ['inspection.py', '--build', str(self.build), '--experiment', '6',
                               '--revision', self.revision]), \
                patch.object(inspector, 'validate_inspection', return_value=self.records) as validated, \
                contextlib.redirect_stdout(io.StringIO()):
            inspector.main()
        build, path, revision, workloads, arms = validated.call_args.args
        self.assertEqual((build, path, revision), (self.build, self.build / 'inspection.json', self.revision))
        self.assertEqual(set(workloads), set(reducer.MANIFEST))
        self.assertEqual(arms, reducer.E6_ARMS)
