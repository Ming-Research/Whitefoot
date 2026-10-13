#!/usr/bin/env python3
"""Count one binary's executed instructions from a callgrind file, per Lua call.

Part of the surviving-call probe in DESIGN.md ("Probe before building");
deleted with the experiment branch. The counting follows the join in
.github/workflows/halo-call-path.yml (callgrind addresses joined to objdump),
so the numbers are comparable with that workflow's B/C/D measurements.

  measure.py run LABEL EXE CALLGRIND_OUT LUA_CALLS OUTDIR
      writes OUTDIR/LABEL.json and OUTDIR/LABEL.arm-<role>.txt (the arm's
      disassembly with executions per Lua call).
  measure.py table OUTDIR LABEL...
      prints the markdown comparison table and the pre-written criteria.

Arms are the functions `...run$instance$<hash>.body.arm.<n>` for the dispatch
arms of the five hot opcodes; clones are the symbols ending `.most`.
"Self" is the instructions executed in the function's own code. "With
callees" adds the inclusive cost callgrind attributes to its calls, leaving
out callees that are dispatch arms or the dispatch body (a part reaches the
next arm by a tail jump, which callgrind may book as a call).
"""
import collections
import json
import os
import re
import subprocess
import sys

ARMS = {25: 'SubRK', 54: 'LtJmpRK', 5: 'GetUpval', 63: 'Call', 65: 'Return'}
STORE = re.compile(r'^(v?mov\w*|stos\w*)$')
DISPATCH = re.compile(r'\.arm\.\d+$|\.body$|\.dispatch$')


def parse_callgrind(cg_out, exe_real):
    names = {'ob': {}, 'fn': {}}

    def resolve(kind, rest):
        m = re.match(r'\((\d+)\)\s*(.*)$', rest)
        if not m:
            return rest
        if m.group(2):
            names[kind][m.group(1)] = m.group(2)
        return names[kind].get(m.group(1), rest)

    cost, fn_min = collections.Counter(), {}
    self_by_fn, callee_incl = collections.Counter(), collections.Counter()
    npos, last, ob, fn, cfn, skip = 1, [0, 0], None, None, None, False
    with open(cg_out) as f:
        for raw in f:
            line = raw.rstrip('\n')
            if line.startswith('positions:'):
                npos = len(line.split()) - 1
                continue
            m = re.match(r'^(ob|cob|fl|fi|fe|cfi|cfl|fn|cfn)=(.*)$', line)
            if m:
                kind = m.group(1)
                if kind == 'ob':
                    ob = os.path.realpath(resolve('ob', m.group(2)))
                elif kind == 'cob':
                    resolve('ob', m.group(2))
                elif kind == 'fn':
                    fn = re.sub(r"'\d+$", '', resolve('fn', m.group(2)))
                elif kind == 'cfn':
                    cfn = re.sub(r"'\d+$", '', resolve('fn', m.group(2)))
                continue
            if line.startswith(('calls=', 'jump=', 'jcnd=')):
                skip = line.startswith('calls=')
                continue
            parts = line.split()
            if not parts or not re.match(r'^([+-]?(0x)?[0-9a-fA-F]+|\*)$', parts[0]):
                continue
            pos = []
            for i, tok in enumerate(parts[:npos]):
                pos.append(last[i] if tok == '*' else last[i] + int(tok, 0) if tok[0] in '+-' else int(tok, 0))
            last[:len(pos)] = pos
            if skip:
                skip = False
                if ob == exe_real and fn is not None and cfn is not None and len(parts) > npos:
                    callee_incl[(fn, cfn)] += int(parts[npos])
                continue
            if ob != exe_real or fn is None or len(parts) <= npos:
                continue
            cost[pos[0]] += int(parts[npos])
            self_by_fn[fn] += int(parts[npos])
            fn_min[fn] = min(fn_min.get(fn, pos[0]), pos[0])
    return cost, fn_min, self_by_fn, callee_incl


def disassemble(exe, sym):
    out = subprocess.run(['objdump', '-d', '--no-show-raw-insn', '-M', 'intel', f'--disassemble={sym}', exe],
                         capture_output=True, text=True).stdout
    rows = []
    for row in out.splitlines():
        m = re.match(r'^\s*([0-9a-f]+):\s*(.*)$', row)
        if m:
            rows.append((int(m.group(1), 16), m.group(2)))
    return rows


def run(label, exe, cg_out, calls, outdir):
    calls = int(calls)
    exe_real = os.path.realpath(exe)
    cost, fn_min, self_by_fn, callee_incl = parse_callgrind(cg_out, exe_real)
    nm = {}
    for row in subprocess.run(['nm', '--defined-only', exe], capture_output=True, text=True).stdout.splitlines():
        bits = row.split(None, 2)
        if len(bits) == 3 and bits[1] in 'tTwW':
            nm.setdefault(bits[2], int(bits[0], 16))
    biases = collections.Counter(fn_min[f] - nm[f] for f in fn_min if f in nm)
    bias = biases.most_common(1)[0][0] if biases else 0
    total = sum(cost.values())
    result = {'label': label, 'lua_calls': calls, 'total': total, 'total_per_call': total / calls,
              'bias': hex(bias), 'arms': {}}

    for number, role in ARMS.items():
        syms = [s for s in nm if re.search(r'\$[0-9a-f]+\.body\.arm\.%d$' % number, s)]
        if len(syms) != 1:
            result['arms'][role] = {'error': f'{len(syms)} symbols match arm {number}: {syms}'}
            continue
        sym = syms[0]
        rows = disassemble(exe, sym)
        executed = sum(cost.get(a + bias, 0) for a, _ in rows)
        callees = {c: v / calls for (f, c), v in callee_incl.items() if f == sym and not DISPATCH.search(c)}
        # The entry block: the instructions before the first control transfer.
        entry_stores, entry_pushes, entry_len = 0, 0, 0
        for _, text in rows:
            mnemonic, _, operands = text.partition(' ')
            if mnemonic.startswith('j') or mnemonic in ('call', 'ret'):
                break
            entry_len += 1
            first = operands.strip().split(',', 1)[0]
            if mnemonic == 'push':
                entry_pushes += 1
            elif STORE.match(mnemonic) and re.match(r'^[a-z ]*\[(rsp|rbp)', first):
                entry_stores += 1
        stores = sum(1 for _, t in rows if STORE.match(t.partition(' ')[0]) and re.match(r'^[a-z ]*\[(rsp|rbp)', t.partition(' ')[2].strip().split(',', 1)[0]))
        result['arms'][role] = {
            'symbol': sym, 'static_instructions': len(rows), 'self_per_call': executed / calls,
            'self_per_call_callgrind_fn': self_by_fn.get(sym, 0) / calls,
            'with_callees_per_call': executed / calls + sum(callees.values()),
            'callees_per_call': dict(sorted(callees.items(), key=lambda kv: -kv[1])),
            'entry_block_instructions': entry_len, 'entry_block_stack_stores': entry_stores,
            'entry_block_pushes': entry_pushes, 'static_stack_stores': stores,
        }
        with open(os.path.join(outdir, f'{label}.arm-{role}.txt'), 'w') as w:
            w.write(f'# {sym}\n# executions per Lua call, address, instruction (entry block: first {entry_len})\n')
            for a, text in rows:
                n = cost.get(a + bias, 0)
                w.write(f'{n / calls:9.3f}  {a:x}: {text}\n')

    clones = {}
    for fname, n in self_by_fn.items():
        if fname.endswith('.most'):
            clones[fname] = n / calls
    result['clones'] = dict(sorted(clones.items(), key=lambda kv: -kv[1]))
    result['clones_self_per_call'] = sum(clones.values())
    result['clones_executed'] = sum(1 for v in clones.values() if v > 0)
    result['clones_defined'] = sum(1 for s in nm if s.endswith('.most'))
    with open(os.path.join(outdir, f'{label}.json'), 'w') as w:
        json.dump(result, w, indent=1)
    print(f'{label}: total per Lua call {result["total_per_call"]:.2f}; '
          + ' '.join(f'{r} {a.get("self_per_call", float("nan")):.1f}' for r, a in result['arms'].items())
          + f'; clones executed {result["clones_executed"]}/{result["clones_defined"]}, self {result["clones_self_per_call"]:.2f}')


def table(outdir, labels):
    data = {}
    for label in labels:
        path = os.path.join(outdir, f'{label}.json')
        if os.path.exists(path):
            data[label] = json.load(open(path))
        else:
            print(f'(no result for {label})')
    labels = [l for l in labels if l in data]

    def arm(d, role, key):
        return d['arms'].get(role, {}).get(key, float('nan'))

    rows = [
        ('Program total, instructions per Lua call', lambda d: d['total_per_call']),
        ('SubRK (arm 25) self', lambda d: arm(d, 'SubRK', 'self_per_call')),
        ('LtJmpRK (arm 54) self', lambda d: arm(d, 'LtJmpRK', 'self_per_call')),
        ('GetUpval (arm 5) self', lambda d: arm(d, 'GetUpval', 'self_per_call')),
        ('SubRK + LtJmpRK + GetUpval', lambda d: sum(arm(d, r, 'self_per_call') for r in ('SubRK', 'LtJmpRK', 'GetUpval'))),
        ('Call (arm 63) self', lambda d: arm(d, 'Call', 'self_per_call')),
        ('Return (arm 65) self', lambda d: arm(d, 'Return', 'self_per_call')),
        ('Call with callees', lambda d: arm(d, 'Call', 'with_callees_per_call')),
        ('Return with callees', lambda d: arm(d, 'Return', 'with_callees_per_call')),
        ('`.most` clones: self instructions per Lua call (all)', lambda d: d['clones_self_per_call']),
        ('`.most` clones executed / defined', lambda d: f"{d['clones_executed']} / {d['clones_defined']}"),
        ('SubRK entry block: stack stores (static)', lambda d: arm(d, 'SubRK', 'entry_block_stack_stores')),
        ('SubRK entry block: pushes (static)', lambda d: arm(d, 'SubRK', 'entry_block_pushes')),
        ('SubRK entry block: instructions (static)', lambda d: arm(d, 'SubRK', 'entry_block_instructions')),
    ]
    out = ['| Measure | ' + ' | '.join(labels) + ' |', '|---|' + '---|' * len(labels)]
    for name, fn in rows:
        cells = []
        for l in labels:
            v = fn(data[l])
            cells.append(v if isinstance(v, str) else (f'{v:.2f}' if isinstance(v, float) else str(v)))
        out.append(f'| {name} | ' + ' | '.join(cells) + ' |')
    out.append('')
    if 'V0' in data:
        v0 = data['V0']
        shown = f"{v0['total_per_call']:.2f}"
        out.append('V0 reproduces B: ' + ('yes, 841.53' if shown == '841.53' else
                   f'!!!!!! NO: V0 is {shown}, B was 841.53; the codegen path differs from lld\'s, read nothing below as B-relative !!!!!!'))

        def trio(d):
            return sum(arm(d, r, 'self_per_call') for r in ('SubRK', 'LtJmpRK', 'GetUpval'))
        for name in ('V1', 'V2', 'V3', 'V2s', 'V2e', 'V2i', 'PGOa', 'PGOb'):
            if name in data:
                d = data[name]
                out.append(f'{name} against V0: three-arm sum {trio(d) - trio(v0):+.2f}, program total {d["total_per_call"] - v0["total_per_call"]:+.2f} per Lua call')
        if 'V1' in data:
            gain = (trio(v0) - trio(data['V1']), v0['total_per_call'] - data['V1']['total_per_call'])
            out.append('Pre-written rejection (V1 lowers neither the three-arm sum nor the total by at least 10): '
                       + ('NOT triggered' if gain[0] >= 10 or gain[1] >= 10 else 'TRIGGERED: the convention premise is rejected'))
        if 'V1' in data and 'V2' in data:
            near = abs(trio(data['V2']) - trio(data['V1'])) <= 3 or trio(data['V2']) <= trio(data['V1'])
            out.append('V2 alone reaches V1\'s three-arm sum within 3 (cost is spill placement, not the convention): '
                       + ('YES' if near else 'no'))
    out.extend(kernel_table(outdir))
    out.extend(criteria(data, outdir))
    print('\n'.join(out))


def read_kernels(outdir):
    """OUTDIR/kernels.tsv: kernel, label, total instructions of one run."""
    path = os.path.join(outdir, 'kernels.tsv')
    got = {}
    if os.path.exists(path):
        for row in open(path):
            bits = row.rstrip('\n').split('\t')
            if len(bits) == 3 and bits[2].isdigit():
                got[(bits[0], bits[1])] = int(bits[2])
    return got


def kernel_table(outdir):
    got = read_kernels(outdir)
    if not got:
        return ['', '(no kernels.tsv: the harm check did not run)']
    labels = sorted({l for _, l in got}, key=lambda l: (l != 'V0', l))
    kernels = sorted({k for k, _ in got})
    out = ['', 'Halo kernels at reduced N, total instructions per run (callgrind "Collected"):', '',
           '| Kernel | ' + ' | '.join(labels) + ' |', '|---|' + '---|' * len(labels)]
    for k in kernels:
        cells = []
        for l in labels:
            v = got.get((k, l))
            base = got.get((k, 'V0'))
            cells.append('n/a' if v is None else f'{v}' + ('' if l == 'V0' or not base else f' ({(v - base) / base * 100:+.3f}%)'))
        out.append(f'| {k} | ' + ' | '.join(cells) + ' |')
    return out


def verdict(ok):
    return 'PASS' if ok else 'FAIL'


def criteria(data, outdir):
    """Each pre-written criterion of DESIGN.md's E1 and E3, quoted, with its number, the measured one and the verdict."""
    out = ['', 'Criteria of the cold-callee study, quoted from DESIGN.md (PASS means the prediction held):', '']
    total = lambda l: data[l]['total_per_call'] if l in data else None
    v0, v2 = total('V0'), total('V2')
    if v0 is not None and v2 is not None:
        out.append(f'Measured here: V0 {v0:.2f}, V2 {v2:.2f}, V2 gain {v0 - v2:.2f} per Lua call (the study quotes 841.53, 806.03, 35.5).')
    s, e, i = total('V2s'), total('V2e'), total('V2i')
    if s is not None:
        out.append('- E1 "V2s gives at least 60% of V2\'s 35.5 instructions per Lua call (at most 820.2)": '
                   f'criterion V2s <= 820.2; measured {s:.2f}; {verdict(s <= 820.2)}')
    if e is not None and i is not None:
        gain = (v0 - e) + (v0 - i)
        out.append('- E1 "V2e plus V2i together at most 40%" (of 35.5, so at most 14.2 of gain; the two gains added, '
                   f'since no variant marks both): criterion V2e gain + V2i gain <= 14.2; measured {v0 - e:.2f} + {v0 - i:.2f} = {gain:.2f}; {verdict(gain <= 14.2)}')
    if e is not None:
        out.append('- E1 "If V2e alone reaches 80% of V2 (at most 813.1), S3 and H are not needed for Halo\'s fib and E2 stops after S1": '
                   f'criterion V2e <= 813.1; measured {e:.2f}; {verdict(e <= 813.1)}'
                   + ('' if e <= 813.1 else ' (so S3 and H stay in play)'))
    got = read_kernels(outdir)
    for k in ('concat', 'string-key'):
        a, b = got.get((k, 'V0')), got.get((k, 'V2'))
        if a and b:
            rise = (b - a) / a * 100
            out.append('- E1 "if V2 raises either kernel\'s instruction count by more than 1%, function-level `cold` on `heap.intern` is confirmed harmful '
                       f'and stays excluded from every rule": {k}: criterion rise <= 1% else harmful; measured V0 {a}, V2 {b}, {rise:+.3f}%; '
                       f'{verdict(rise <= 1)} (PASS = not harmful on this kernel)')
        else:
            out.append(f'- E1 harm check on {k}: no V0/V2 count in kernels.tsv')
        c = got.get((k, 'V2i'))
        if a and c:
            out.append(f'  (additionally {k} under V2i, `heap.intern` alone: {(c - a) / a * 100:+.3f}%)')
    pa, pb = total('PGOa'), total('PGOb')
    if pa is None and pb is None:
        note = os.path.join(outdir, 'e3-not-applicable.txt')
        out.append('- E3: ' + (open(note).read().strip() if os.path.exists(note) else 'no PGO result'))
    if pa is not None:
        out.append('- E3 "(a) at or below V2\'s 806.03": '
                   f'criterion PGOa <= 806.03; measured {pa:.2f}; {verdict(pa <= 806.03)}')
    if pb is not None:
        out.append('- E3 "If (b) recovers less than half of V2\'s gain (above 823.8), a profile does not transfer across Lua workloads, '
                   'which confirms that `slow`\'s coldness is a property of the input and that P is not a default build mode": '
                   f'criterion PGOb > 823.8 confirms; measured {pb:.2f}; '
                   + ('CONFIRMED (does not transfer)' if pb > 823.8 else 'NOT confirmed (a profile from other kernels recovers at least half)'))
    return out


if __name__ == '__main__':
    if sys.argv[1] == 'run':
        run(*sys.argv[2:7])
    else:
        table(sys.argv[2], sys.argv[3:])
