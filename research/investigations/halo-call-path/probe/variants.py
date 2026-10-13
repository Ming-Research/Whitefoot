#!/usr/bin/env python3
"""Rewrite B's post-LTO Halo module into the four probe variants.

Part of the surviving-call probe in DESIGN.md ("Probe before building"). The
input is lld's `*.precodegen.ll` of Halo's bench host built by compiler B (no
call entries): the module as it reaches host code generation, with every
inlining decision already made. Deleted with the experiment branch.

  usage: variants.py precodegen.ll OUTDIR

writes OUTDIR/v0.ll .. v3.ll and OUTDIR/variants-report.txt.

Terms. A part is a function defined `preserve_nonecc` (a dispatch arm or
other split piece). A surviving call is a `call fastcc @f(` that is not a
tail call, inside a part: the call the host did not inline.

  V0  the input, unchanged (control).
  V1  every function f that a part calls by a surviving call gets a clone
      `@f.most` defined `preserve_mostcc` instead of `fastcc`, the surviving
      call sites in parts call `preserve_mostcc @f.most`, and inside every
      clone each `call fastcc @g(` (plain, `tail` or `musttail`, since a
      musttail callee must share the caller's convention) calls
      `preserve_mostcc @g.most` instead, cloning g too, transitively.
      Attributes, call-site attribute groups and bodies are copied intact.
      Not touched: calls to external C functions or declarations (no
      definition to clone), `tail`/`musttail` calls inside parts, and calls
      from the original (non-clone, non-part) functions, which keep fastcc.
  V2  the `cold` function attribute added to the slow callees `slow`,
      `heap.intern`, `unwind` and `restore_stopped_stack` (matched by exact
      mangled name below); calling conventions unchanged.
  V3  V1 and V2 together (the clones of the slow callees are cold too).
"""
import re
import sys

COLD = re.compile(r'^wf_package\.halo\.(vm\.slow\$instance\$[0-9a-f]+|heap\.intern|vm\.unwind|vm\.restore_stopped_stack)$')
LINKAGE = {'internal', 'private', 'linkonce_odr', 'weak', 'weak_odr', 'dso_local', 'hidden', 'protected', 'external', 'linkonce', 'common', 'available_externally'}
NAME = re.compile(r'@("[^"]+"|[\w.$-]+)\(')
CALL = re.compile(r'(?P<pre>(?:musttail |tail |notail )?call) fastcc (?P<mid>[^@]*?)@(?P<name>"[^"]+"|[\w.$-]+)\(')
MOST = '.most'


def bare(name):
    return name[1:-1] if name.startswith('"') else name


def with_suffix(name):
    return name[:-1] + MOST + '"' if name.startswith('"') else name + MOST


def header_info(line):
    words = line.split()[1:]
    i = 0
    while i < len(words) and words[i] in LINKAGE:
        i += 1
    cc = words[i] if i < len(words) and words[i].endswith('cc') else 'ccc'
    m = NAME.search(line)
    return cc, m.group(1)


def read_functions(lines):
    funcs = {}
    start = None
    for i, line in enumerate(lines):
        if line.startswith('define '):
            start = i
        elif line == '}' and start is not None:
            cc, name = header_info(lines[start])
            funcs[bare(name)] = (start, i, cc, name)
            start = None
    return funcs


def rewrite_calls(line, defined_fastcc, only_plain, hits, kinds):
    def sub(m):
        callee = bare(m.group('name'))
        pre = m.group('pre')
        if callee not in defined_fastcc or (only_plain and pre != 'call'):
            return m.group(0)
        hits.add(callee)
        kinds[pre] = kinds.get(pre, 0) + 1
        return f"{pre} preserve_mostcc {m.group('mid')}@{with_suffix(m.group('name'))}("
    return CALL.sub(sub, line)


def make_v1(lines, funcs, report):
    fastcc = {n for n, f in funcs.items() if f[2] == 'fastcc'}
    parts = [n for n, f in funcs.items() if f[2] == 'preserve_nonecc']
    out = list(lines)
    wanted, part_sites = set(), {}
    for n in parts:
        s, e, _, _ = funcs[n]
        for i in range(s + 1, e):
            hits = set()
            new = rewrite_calls(out[i], fastcc, True, hits, part_sites)
            if hits:
                out[i] = new
                wanted |= hits
    report.append(f'parts: {len(parts)}')
    report.append(f'surviving call sites rewritten in parts: {sum(part_sites.values())}')
    report.append(f'distinct first-level callees (clones seeded): {len(wanted)}')
    clones, done, queue, inner = [], set(), sorted(wanted), {}
    while queue:
        f = queue.pop()
        if f in done:
            continue
        done.add(f)
        s, e, _, rawname = funcs[f]
        body = lines[s:e + 1]
        header = body[0].replace(' fastcc ', ' preserve_mostcc ', 1)
        header = header.replace('@' + rawname + '(', '@' + with_suffix(rawname) + '(', 1)
        new_body = [header]
        for line in body[1:]:
            hits = set()
            new_body.append(rewrite_calls(line, fastcc, False, hits, inner))
            for g in hits:
                if g not in done:
                    queue.append(g)
        clones.append((f, new_body))
    report.append(f'clones: {len(clones)} (first level {len(wanted)}, transitive {len(clones) - len(wanted)})')
    report.append(f'calls rewritten inside clones: {sum(inner.values())} {dict(inner)}')
    for f, body in sorted(clones):
        out.append('')
        out.extend(body)
    return out, [f for f, _ in clones]


def add_cold(lines, report):
    out, matched = [], []
    for line in lines:
        if line.startswith('define '):
            _, raw = header_info(line)
            base = bare(raw)
            if base.endswith(MOST):
                base = base[:-len(MOST)]
            if COLD.match(base):
                new = re.sub(r'(\)\s+(?:unnamed_addr\s+)?)(#\d+)', r'\1cold \2', line, count=1)
                if new == line:
                    raise SystemExit(f'no attribute group to put cold before: {line[:200]}')
                matched.append(bare(raw))
                line = new
        out.append(line)
    report.append(f'cold added to {len(matched)} definitions:')
    report.extend('  ' + n for n in matched)
    return out


def main():
    src, outdir = sys.argv[1:3]
    lines = open(src).read().split('\n')
    funcs = read_functions(lines)
    report = [f'input {src}: {len(funcs)} definitions']
    unmatched = sum(1 for l in lines if ' call fastcc ' in l and not CALL.search(l))
    report.append(f'call fastcc lines the call pattern does not match (left alone): {unmatched}')
    v1, _ = make_v1(lines, funcs, report)
    report.append('V2/V3 cold:')
    v2 = add_cold(lines, report)
    v3 = add_cold(v1, report)
    for name, text in (('v0', lines), ('v1', v1), ('v2', v2), ('v3', v3)):
        with open(f'{outdir}/{name}.ll', 'w') as f:
            f.write('\n'.join(text))
        report.append(f'{name}.ll: {len(text)} lines')
    with open(f'{outdir}/variants-report.txt', 'w') as f:
        f.write('\n'.join(report) + '\n')
    print('\n'.join(report))


main()
