#!/usr/bin/env python3
"""V2 causal probe: sink surviving arm-local pointer initializers after LTO.

This is an experiment-only textual rewrite, not an LLVM parser or compiler
optimization. The native LLVM verifier remains mandatory before codegen.
Supported functions have explicit block labels and an acyclic reachable CFG
using br/switch/ret/unreachable. A slot has one entry alloca and one entry
initializer from a function argument; its other uses are direct pointer loads
or direct call arguments. Initialization moves to first-use blocks only when
every incoming path is uninitialized. Mixed-state joins are unavailable.
Consequently a call may modify/capture the slot: no path initializes it twice,
and no alias exists before its first consumer. Unsupported forms fail closed.

Usage: sink_initializers.py INPUT.ll OUTPUT.ll REPORT.json
       sink_initializers.py --self-test   # CI only
"""

import json
import re
import sys
import unittest
from pathlib import Path


VALUE = r'%[-a-zA-Z$._0-9]+'
# LLVM quotes names copied from inlined functions even when their characters
# also admit the unquoted spelling. Other quoted forms remain unsupported.
LABEL = r'(?:[-a-zA-Z$._0-9]+|"[-a-zA-Z$._0-9]+")'
TARGET = re.compile(r'^wf_package\.halo\.vm\.run\$instance\$[0-9a-f]+\.body\.arm\.(25|54|5)$')
DEFINE = re.compile(r'^define\b.*?@("[^"]+"|[-a-zA-Z$._0-9]+)\(')
PIN = re.compile(r'^\s*(%wf\.pin\.[-a-zA-Z$._0-9]+)\s*=\s*alloca\s+ptr(?:\s*,\s*align\s+\d+)?\s*$')


class Unavailable(Exception):
    pass


def code(line):
    return line.split(';', 1)[0].strip()


def token_uses(line, name):
    return len(re.findall(re.escape(name) + r'(?![-a-zA-Z$._0-9])', line))


def direct_arguments(text):
    callee = re.search(r'@(?:"[^"]+"|[-a-zA-Z$._0-9]+)\(', text)
    if not callee or '[' in text:
        return None
    start = callee.end()
    depth = 1
    arguments = []
    for index in range(start, len(text)):
        char = text[index]
        if char == '(':
            depth += 1
        elif char == ')':
            depth -= 1
            if depth == 0:
                arguments.append(text[start:index].strip())
                return arguments
        elif char == ',' and depth == 1:
            arguments.append(text[start:index].strip())
            start = index + 1
    return None


def blocks_of(lines):
    blocks = {}
    current = None
    for index, line in enumerate(lines[1:-1], 1):
        text = code(line)
        if not text:
            continue
        match = re.fullmatch('(' + LABEL + r'):', text)
        if match:
            current = match[1].strip('"')
            if current in blocks:
                raise Unavailable('duplicate block label')
            blocks[current] = []
        elif current is None:
            raise Unavailable('implicit or unsupported entry block label')
        else:
            blocks[current].append((index, text))
    if not blocks:
        raise Unavailable('no blocks')
    successors = {}
    for name, instructions in blocks.items():
        terminators = [i for i, (_, text) in enumerate(instructions)
                       if re.match(r'^(br|switch|ret|unreachable)\b', text)]
        if len(terminators) != 1:
            raise Unavailable('unsupported or missing terminator in ' + name)
        start = terminators[0]
        terminal = ' '.join(text for _, text in instructions[start:])
        if terminal.startswith('switch '):
            if ']' not in terminal:
                raise Unavailable('unterminated switch in ' + name)
        elif start != len(instructions) - 1:
            raise Unavailable('instructions after terminator in ' + name)
        edges = [edge.strip('"') for edge in re.findall(
            r'\blabel\s+%(' + LABEL + r')(?![-a-zA-Z$._0-9])', terminal)]
        if len(edges) != len(re.findall(r'\blabel\s+%', terminal)):
            raise Unavailable('unsupported successor label in ' + name)
        expected = 1 if terminal.startswith('br label ') else 2
        if terminal.startswith('br ') and len(edges) != expected:
            raise Unavailable('unsupported branch in ' + name)
        if terminal.startswith('switch ') and not edges:
            raise Unavailable('unsupported switch in ' + name)
        if any(edge not in blocks for edge in edges):
            raise Unavailable('unknown successor in ' + name)
        successors[name] = list(dict.fromkeys(edges))
    entry = next(iter(blocks))
    reachable = set()
    pending = [entry]
    while pending:
        name = pending.pop()
        if name not in reachable:
            reachable.add(name)
            pending.extend(successors[name])
    indegree = {name: 0 for name in blocks if name in reachable}
    for name in indegree:
        for successor in successors[name]:
            indegree[successor] += 1
    ready = [name for name in indegree if indegree[name] == 0]
    order = []
    while ready:
        name = ready.pop(0)
        order.append(name)
        for successor in successors[name]:
            indegree[successor] -= 1
            if not indegree[successor]:
                ready.append(successor)
    if len(order) != len(reachable):
        raise Unavailable('cyclic reachable CFG')
    return blocks, successors, order, entry


def plan_slot(lines, slot, allocation, blocks, successors, order, entry):
    report = {'name': slot, 'status': 'unavailable', 'insertion_blocks': []}
    occurrences = [(block, index, text) for block, body in blocks.items()
                   for index, text in body if token_uses(text, slot)]
    initializers = []
    consumers = {}
    for block, index, text in occurrences:
        if index == allocation:
            if block != entry:
                raise Unavailable('alloca is outside entry')
            continue
        store = re.fullmatch(r'store ptr (' + VALUE + r'), ptr ' + re.escape(slot)
                             + r'(?:, align \d+)?(?:, ![-\w.]+ !\d+)*', text)
        if store:
            initializers.append((block, index, store[1]))
            continue
        # Lifetime and debug constructs have special placement/metadata rules;
        # reject instead of pretending a partial textual parser proves them.
        if '@llvm.lifetime.' in text or '@llvm.dbg.' in text or text.startswith('#dbg_'):
            raise Unavailable('slot has lifetime/debug uses requiring native LLVM analysis')
        load = re.fullmatch(VALUE + r' = load ptr, ptr ' + re.escape(slot)
                            + r'(?:, align \d+)?(?:, ![-\w.]+ !\d+)*', text)
        call = re.match(r'^(?:' + VALUE + r' = )?(?:tail |notail )?call\b', text)
        # Direct, single-line call; pointer argument attributes are retained.
        # Other slot operands, bundles, expressions and multiline forms reject.
        safe_call = False
        if call:
            arguments = direct_arguments(text)
            if arguments is not None:
                uses = [arg for arg in arguments if token_uses(arg, slot)]
                safe_call = (len(uses) == token_uses(text, slot)
                             and all(re.fullmatch(r'ptr(?:\s+[^%@]+)?\s+' + re.escape(slot), arg)
                                     for arg in uses))
        if not load and not safe_call:
            raise Unavailable('unsupported slot use at function line ' + str(index + 1))
        if block not in order:
            raise Unavailable('slot consumer in unreachable block')
        consumers.setdefault(block, index)
    if len(initializers) != 1:
        raise Unavailable('expected one initializer; found ' + str(len(initializers)))
    block, original, incoming = initializers[0]
    if block != entry or original <= allocation:
        raise Unavailable('initializer is not after entry alloca')
    if not token_uses(lines[0], incoming):
        raise Unavailable('initializer value is not an incoming function argument')
    if entry in consumers and consumers[entry] <= original:
        raise Unavailable('entry consumer precedes initializer')
    report['initializer_line'] = original + 1
    report['initializer_operand'] = incoming
    if not consumers:
        report.update(status='no-op', reason='slot has no surviving consumers')
        return report, None, []
    states = {name: set() for name in order}
    states[entry].add(False)
    insertions = []
    for name in order:
        state = states[name]
        if name in consumers:
            if len(state) != 1:
                raise Unavailable('initialized/uninitialized paths join at consumer block ' + name)
            if state == {False}:
                insertions.append((consumers[name], lines[original]))
                report['insertion_blocks'].append(name)
                state = {True}
        for successor in successors[name]:
            states[successor].update(state)
    if entry in report['insertion_blocks']:
        report.update(status='no-op', reason='entry has a surviving consumer')
        report['insertion_blocks'] = []
        return report, None, []
    report.update(status='changed', reason='initialize once on each first-use path')
    return report, original, insertions


def transform(source):
    lines = source.splitlines(keepends=True)
    reports = []
    replacements = []
    seen = set()
    duplicate_suffixes = set()
    index = 0
    while index < len(lines):
        match = DEFINE.match(lines[index])
        name = match[1].strip('"') if match else ''
        target = TARGET.fullmatch(name)
        if not target:
            index += 1
            continue
        end = index + 1
        while end < len(lines) and lines[end].strip() != '}':
            end += 1
        report = {'name': name, 'status': 'no-op', 'slots': []}
        reports.append(report)
        if target[1] in seen:
            duplicate_suffixes.add(target[1])
        seen.add(target[1])
        body = lines[index:end + 1]
        try:
            if end == len(lines):
                raise Unavailable('unterminated function')
            pins = [(i, PIN.fullmatch(code(line))) for i, line in enumerate(body)
                    if re.search(r'%wf\.pin\.[-a-zA-Z$._0-9]+\s*=\s*alloca\b', code(line))]
            if any(pin is None for _, pin in pins):
                raise Unavailable('unsupported pin alloca shape')
            if not pins:
                report['reason'] = 'no surviving addressable pin alloca'
            else:
                blocks, successors, order, entry = blocks_of(body)
                removals = set()
                before = {}
                for allocation, pin in pins:
                    try:
                        slot_report, removal, additions = plan_slot(
                            body, pin[1], allocation, blocks, successors, order, entry)
                    except Unavailable as error:
                        report['slots'].append({'name': pin[1], 'status': 'unavailable',
                                                'reason': str(error), 'insertion_blocks': []})
                        raise
                    report['slots'].append(slot_report)
                    if removal is not None:
                        removals.add(removal)
                    for position, store in additions:
                        before.setdefault(position, []).append(store)
                if removals:
                    report['status'] = 'changed'
                    changed = ''.join(''.join(before.get(i, [])) + ('' if i in removals else line)
                                      for i, line in enumerate(body))
                    replacements.append((index, end + 1, changed))
        except Unavailable as error:
            report.update(status='unavailable', reason=str(error))
        index = end + 1
    missing = sorted({'25', '54', '5'} - seen)
    # Nonlocal re-entry is not represented by successor edges. Refuse even an
    # unrelated returns_twice declaration rather than infer a call graph here.
    unsupported_module_features = (['returns_twice'] if re.search(r'\breturns_twice\b', source) else [])
    unavailable = (bool(missing or duplicate_suffixes or unsupported_module_features)
                   or any(r['status'] == 'unavailable' for r in reports))
    result = {'status': 'unavailable' if unavailable else ('changed' if replacements else 'no-op'),
              'functions': reports, 'changed_functions': [], 'missing_arm_suffixes': missing,
              'ambiguous_arm_suffixes': sorted(duplicate_suffixes),
              'unsupported_module_features': unsupported_module_features}
    if unavailable:
        result['reason'] = 'rewrite unavailable; output is byte-for-byte input; no planned edit applied'
        for report in reports:
            report['applied'] = False
        return source, result
    for begin, end, replacement in reversed(replacements):
        lines[begin:end] = [replacement]
    result['changed_functions'] = [r['name'] for r in reports if r['status'] == 'changed']
    for report in reports:
        report['applied'] = report['status'] == 'changed'
    return ''.join(lines), result


def fixture(body):
    return ('declare void @consume(ptr)\n' + ''.join(
        'define internal void @wf_package.halo.vm.run$instance$abc.body.arm.' + suffix
        + '(ptr %incoming, i1 %condition) {\n' + (body if suffix == '25' else 'entry:\n  ret void\n')
        + '}\n' for suffix in ('25', '54', '5')))


class Fixtures(unittest.TestCase):
    entry = 'entry:\n  %wf.pin.8.1 = alloca ptr, align 8\n  store ptr %incoming, ptr %wf.pin.8.1, align 8\n'

    def test_disjoint_first_consumers(self):
        source = fixture(self.entry + '  br i1 %condition, label %hot, label %cold\n'
                         'hot:\n  ret void\ncold:\n  br i1 %condition, label %left, label %right\n'
                         'left:\n  call void @consume(ptr %wf.pin.8.1)\n  ret void\n'
                         'right:\n  %p = load ptr, ptr %wf.pin.8.1, align 8\n  ret void\n')
        output, report = transform(source)
        self.assertEqual(report['status'], 'changed')
        self.assertEqual(report['functions'][0]['slots'][0]['insertion_blocks'], ['left', 'right'])
        self.assertEqual(output.count('store ptr %incoming'), 2)
        self.assertNotIn('store ptr', output.split('cold:')[0])

    def test_writing_call_not_reinitialized(self):
        source = fixture(self.entry + '  br label %cold\ncold:\n'
                         '  call void @consume(ptr %wf.pin.8.1)\n  br label %later\nlater:\n'
                         '  %p = load ptr, ptr %wf.pin.8.1\n  ret void\n')
        output, report = transform(source)
        self.assertEqual(report['status'], 'changed')
        self.assertEqual(output.count('store ptr %incoming'), 1)
        self.assertNotIn('store ptr', output.split('later:')[1])

    def test_attributed_call_and_store(self):
        source = fixture(self.entry.replace('ptr %wf.pin.8.1, align 8\n',
                                           'ptr %wf.pin.8.1, align 8, !tbaa !0\n')
                         + '  br label %cold\ncold:\n'
                         '  call void @consume(ptr noundef nonnull align 8 dereferenceable(8) captures(none) %wf.pin.8.1)\n'
                         '  ret void\n') + '!0 = !{!1, !1, i64 0}\n!1 = !{!"pointer", !2}\n!2 = !{!"root"}\n'
        output, report = transform(source)
        self.assertEqual(report['status'], 'changed')
        self.assertIn('cold:\n  store ptr %incoming, ptr %wf.pin.8.1, align 8, !tbaa !0', output)

    def test_entry_consumer_noop(self):
        source = fixture(self.entry + '  call void @consume(ptr %wf.pin.8.1)\n  ret void\n')
        self.assertEqual(transform(source)[0], source)
        self.assertEqual(transform(source)[1]['status'], 'no-op')

    def test_initialized_join_keeps_call_writes(self):
        source = fixture(self.entry + '  br i1 %condition, label %left, label %right\n'
                         'left:\n  call void @consume(ptr %wf.pin.8.1)\n  br label %join\n'
                         'right:\n  call void @consume(ptr %wf.pin.8.1)\n  br label %join\n'
                         'join:\n  %p = load ptr, ptr %wf.pin.8.1\n  ret void\n')
        output, report = transform(source)
        self.assertEqual(report['status'], 'changed')
        self.assertEqual(output.count('store ptr %incoming'), 2)
        self.assertNotIn('store ptr', output.split('join:')[1])

    def test_mixed_join_unavailable(self):
        source = fixture(self.entry + '  br i1 %condition, label %used, label %join\nused:\n'
                         '  call void @consume(ptr %wf.pin.8.1)\n  br label %join\njoin:\n'
                         '  %p = load ptr, ptr %wf.pin.8.1\n  ret void\n')
        self.assertEqual(transform(source)[1]['status'], 'unavailable')
        self.assertEqual(transform(source)[0], source)

    def test_quoted_inlined_exit(self):
        # V0 arm.54 bb11.i branches to this quoted, inlined-function exit.
        exit_name = '"wf_package.halo.vm.instruction_lt_jmp_rk$instance$2ead40aebe81bc32.exit"'
        source = fixture(self.entry + '  br i1 %condition, label %hot, label %bb11.i\n'
                         'hot:\n  ret void\nbb11.i:\n'
                         '  call void @consume(ptr %wf.pin.8.1)\n'
                         '  br label %' + exit_name + '\n' + exit_name + ':\n'
                         '  %p = load ptr, ptr %wf.pin.8.1, align 8\n  ret void\n')
        output, report = transform(source)
        self.assertEqual(report['status'], 'changed')
        self.assertEqual(report['functions'][0]['slots'][0]['insertion_blocks'], ['bb11.i'])
        self.assertEqual(output.count('store ptr %incoming'), 1)
        self.assertNotIn('store ptr', output.split(exit_name + ':')[1])
        # A bypass must not vanish from the CFG merely because it is quoted:
        # the exit would then be reached both before and after a writing call.
        bypass = source.replace('label %hot, label %bb11.i',
                                'label %' + exit_name + ', label %bb11.i')
        self.assertEqual(transform(bypass)[1]['status'], 'unavailable')
        self.assertIn('initialized/uninitialized paths join',
                      transform(bypass)[1]['functions'][0]['reason'])
        self.assertEqual(transform(bypass)[0], bypass)

    def test_unsupported_successor_not_silently_dropped(self):
        source = fixture(self.entry + '  br i1 %condition, label %cold, label %"unsupported name"\n'
                         'cold:\n  call void @consume(ptr %wf.pin.8.1)\n  ret void\n'
                         '"unsupported name":\n  ret void\n')
        self.assertEqual(transform(source)[1]['status'], 'unavailable')
        self.assertIn('unsupported successor label', transform(source)[1]['functions'][0]['reason'])
        self.assertEqual(transform(source)[0], source)

    def test_missing_or_multiple_initializer(self):
        for count in (0, 2):
            source = fixture('entry:\n  %wf.pin.8.1 = alloca ptr\n'
                             + '  store ptr %incoming, ptr %wf.pin.8.1\n' * count
                             + '  br label %cold\ncold:\n  call void @consume(ptr %wf.pin.8.1)\n  ret void\n')
            self.assertEqual(transform(source)[1]['status'], 'unavailable')
            self.assertEqual(transform(source)[0], source)

    def test_alias_and_cycle_unavailable(self):
        for tail in ('  %alias = getelementptr i8, ptr %wf.pin.8.1, i64 0\n  ret void\n',
                     '  call void @consume(ptr %wf.pin.8.1)\n  br label %cold\n'):
            source = fixture(self.entry + '  br label %cold\ncold:\n' + tail)
            self.assertEqual(transform(source)[1]['status'], 'unavailable')

    def test_eliminated_slot_noop(self):
        source = fixture('entry:\n  ret void\n')
        self.assertEqual(transform(source)[1]['status'], 'no-op')

    def test_ambiguous_arm_unavailable(self):
        source = fixture('entry:\n  ret void\n')
        source += ('define internal void @wf_package.halo.vm.run$instance$def.body.arm.25(ptr %incoming) {\n'
                   'entry:\n  ret void\n}\n')
        self.assertEqual(transform(source)[1]['ambiguous_arm_suffixes'], ['25'])
        self.assertEqual(transform(source)[1]['status'], 'unavailable')


def main():
    if sys.argv[1:] == ['--self-test']:
        unittest.main(argv=[sys.argv[0]])
        return
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    source_path, output_path, report_path = map(Path, sys.argv[1:])
    if len({path.resolve() for path in (source_path, output_path, report_path)}) != 3:
        raise SystemExit('input, output and report paths must differ')
    source = source_path.read_bytes().decode('utf-8')
    output, report = transform(source)
    output_path.write_bytes(output.encode('utf-8'))
    report_path.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    if report['status'] == 'unavailable':
        raise SystemExit(2)


if __name__ == '__main__':
    main()
