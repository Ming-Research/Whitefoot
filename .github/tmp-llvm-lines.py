# Temporary (wf-gate-budget-noise): removed with tmp-llvm-lines.yml.
import re, sys, glob, collections

def decode(sym):
    # Legacy Rust mangling: _ZN then (length name)* then 17h<hash>E.
    m = re.match(r'_ZN(.*)E$', sym)
    if not m:
        return sym
    body, parts = m.group(1), []
    while body:
        n = re.match(r'(\d+)', body)
        if not n:
            break
        k = int(n.group(1)); s = len(n.group(1))
        parts.append(body[s:s + k]); body = body[s + k:]
    if parts and re.fullmatch(r'h[0-9a-f]{16}', parts[-1]):
        parts.pop()
    return '::'.join(p.replace('$LT$', '<').replace('$GT$', '>').replace('$u20$', ' ').replace('..', '::') for p in parts)

files = [f for f in glob.glob(sys.argv[1] + '/**/*.ll', recursive=True)]
big = max(files, key=lambda f: __import__('os').path.getsize(f))
print('file:', big)
count = collections.Counter(); lines = collections.Counter()
total = 0
current = None
for line in open(big, errors='replace'):
    if line.startswith('define '):
        m = re.search(r'@(\S+?)\(', line)
        current = decode(m.group(1)) if m else '?'
        count[current] += 1; lines[current] += 1; total += 1
    elif current is not None:
        lines[current] += 1; total += 1
        if line.startswith('}'):
            current = None
print('total function lines:', total, 'distinct paths:', len(count))
def block(title, pred):
    sel = [(lines[k], count[k], k) for k in count if pred(k)]
    print('\n###', title, '- paths', len(sel), 'copies', sum(c for _, c, _ in sel), 'lines', sum(l for l, _, _ in sel))
for pat in ('with_resolved_semantics_inputs', 'with_semantics', 'with_resolved_semantics', 'with_semantics_dark', 'with_resolution', 'semantic::tests'):
    block(pat, lambda k, p=pat: p in k)
def prefix(k, n):
    return '::'.join(k.split('::')[:n])
for depth in (3, 4):
    agg = collections.Counter(); cp = collections.Counter()
    for k in count:
        agg[prefix(k, depth)] += lines[k]; cp[prefix(k, depth)] += count[k]
    print(f'\n### top 40 prefixes of depth {depth} by lines (lines, copies, prefix)\n```text')
    for k, l in agg.most_common(40):
        print(f'{l:9d} {cp[k]:6d}  {k[:130]}')
    print('```')
print('\n### top 60 paths by total lines (lines, copies, path)\n```text')
for l, c, k in sorted(((lines[k], count[k], k) for k in count), reverse=True)[:60]:
    print(f'{l:9d} {c:6d}  {k[:150]}')
print('```')
print('\n### top 30 paths by copies\n```text')
for c, l, k in sorted(((count[k], lines[k], k) for k in count), reverse=True)[:30]:
    print(f'{c:6d} {l:9d}  {k[:150]}')
print('```')
