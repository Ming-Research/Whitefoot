"""Raw-IR dispatch controls, discovered by the hosted unittest invocation."""
import unittest

from identity import IdentityError, dispatch_proof, ir_edges, parse_ir


WRAPPER = '''define i64 @wf_bench_micro(i64 %n, i64 %extent) {
entry:
  %pool = call i32 @wf__par_pool_active()
  %active = icmp ne i32 %pool, 0
  br i1 %active, label %par, label %seq
par:
  %a = call i64 @wf_body(i64 %n, i64 %extent)
  ret i64 %a
seq:
  %b = call i64 @wf__par_seq_body(i64 %n, i64 %extent)
  ret i64 %b
}
'''


def region(predicate='  %cheap = icmp ult i64 %extent, 21429', allocation='', large=None):
    large = large or '''large:
  switch i1 %cheap, label %invalid [
    i1 1, label %done
    i1 0, label %again
  ]
again:
  br label %large
invalid:
  unreachable
done:
  ret i64 %n'''
    return f'''define i64 @wf_body(i64 %n, i64 %extent) {{
par.region.entry:
{allocation}
{predicate}
  br i1 %cheap, label %par.region.sequential, label %large
par.region.sequential:
  %result = call i64 @wf__par_seq_body(i64 %n, i64 %extent)
  ret i64 %result
{large}
}}
'''


class IrDispatchTests(unittest.TestCase):
    def test_multiline_switch_retains_default_and_every_case_edge(self):
        blocks = parse_ir(region())['wf_body']
        edges = ir_edges(blocks)
        self.assertEqual(edges['large'], ['invalid', 'done', 'again'])
        self.assertEqual(edges['again'], ['large'])
        proof = dispatch_proof(parse_ir(WRAPPER + region()), 'wf_bench_micro')
        self.assertEqual(proof['pool_decisions_per_timed_call'], 1)
        self.assertEqual(proof['regions'][0]['function'], 'wf_body')
        self.assertTrue(proof['regions'][0]['outside_loops'])

    def test_private_entry_slot_unused_by_predicate_does_not_hide_region(self):
        source = region(allocation='  %slot = alloca ptr, align 8', large='large:\n  ret i64 %n')
        proof = dispatch_proof(parse_ir(WRAPPER + source), 'wf_bench_micro')
        self.assertEqual(len(proof['regions']), 1)
        self.assertEqual(proof['regions'][0]['decisions_per_active_timed_call'], 1)

    def test_entry_slot_result_cannot_feed_predicate(self):
        for predicate in ('  %cheap = icmp ne ptr %slot, null',
                          '  %address = ptrtoint ptr %slot to i64\n  %cheap = icmp ult i64 %address, 10',
                          '  %loaded = load ptr, ptr %slot\n  %cheap = icmp ne ptr %loaded, null'):
            with self.assertRaisesRegex(IdentityError, 'uses private entry allocation'):
                dispatch_proof(parse_ir(WRAPPER + region(predicate, '  %slot = alloca ptr, align 8', large='large:\n  ret i64 %n')), 'wf_bench_micro')

    def test_source_load_or_store_in_predicate_still_fails(self):
        for predicate in ('  %loaded = load i64, ptr %source\n  %cheap = icmp ult i64 %loaded, 10',
                          '  store i64 %extent, ptr %source\n  %cheap = icmp ult i64 %extent, 10'):
            with self.assertRaisesRegex(IdentityError, 'observes memory'):
                dispatch_proof(parse_ir(WRAPPER + region(predicate, large='large:\n  ret i64 %n')), 'wf_bench_micro')

    def test_switch_case_back_to_region_entry_reveals_dispatch_cycle(self):
        large = '''large:
  switch i1 %cheap, label %done [
    i1 0, label %par.region.entry
    i1 1, label %done
  ]
done:
  ret i64 %n'''
        with self.assertRaisesRegex(IdentityError, 'not acyclic function entry'):
            dispatch_proof(parse_ir(WRAPPER + region(large=large)), 'wf_bench_micro')

    def test_malformed_missing_and_duplicate_switch_cases_fail_closed(self):
        valid = region()
        variants = (valid.replace('i1 0, label %again', 'i64 0, label %again'),
                    valid.replace('i1 0, label %again', 'i1 1, label %again'),
                    valid.replace('i1 0, label %again', 'i1 true, label %again'),
                    valid.replace('i1 0, label %again', 'i1 -1, label %again'),
                    valid.replace('i1 0, label %again', 'i1 2, label %again'),
                    valid.replace('i1 0, label %again', 'i1 0, label %missing'),
                    valid.replace('  ]\n', ''),
                    valid.replace('switch i1 %cheap, label %invalid [', 'switch i1 %cheap, label %invalid'),
                    valid.replace('i1 0, label %again', 'i1 0, label %again trailing'))
        for source in variants:
            with self.assertRaises(IdentityError):
                ir_edges(parse_ir(source)['wf_body'])

    def test_constant_switch_selectors_require_matching_type_and_range(self):
        for selector in ('i64 true', 'i64 false', 'i1 2', 'i1 -2'):
            source = region().replace('switch i1 %cheap', 'switch ' + selector)
            with self.assertRaisesRegex(IdentityError, 'switch selector'):
                ir_edges(parse_ir(source)['wf_body'])
        for selector in ('i1 true', 'i1 false', 'i1 1', 'i1 0', 'i1 -1'):
            source = region().replace('switch i1 %cheap', 'switch ' + selector)
            self.assertEqual(ir_edges(parse_ir(source)['wf_body'])['large'],
                             ['invalid', 'done', 'again'])


if __name__ == '__main__':
    unittest.main()
