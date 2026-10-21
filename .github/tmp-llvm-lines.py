# Temporary (wf-gate-budget-noise): removed with tmp-llvm-lines.yml.
import re, sys, glob, os, collections

def extract(root, out):
    files = glob.glob(root + '/**/*.ll', recursive=True)
    big = max(files, key=os.path.getsize)
    lines = collections.Counter(); count = collections.Counter()
    current = None
    for line in open(big, errors='replace'):
        if line.startswith('define '):
            m = re.search(r'@("?[^\s(]+"?)\(', line)
            current = m.group(1).strip('"') if m else '?'
            count[current] += 1; lines[current] += 1
        elif current is not None:
            lines[current] += 1
            if line.startswith('}'):
                current = None
    with open(out, 'w') as f:
        for k in lines:
            f.write(f'{k}\t{lines[k]}\n')
    print('file:', big, 'symbols:', len(lines), 'lines:', sum(lines.values()))

def key(name):
    i = name.find('::<')
    return name[:i] if i >= 0 else name

def report(tsv, names):
    syms = [l.rstrip('\n').split('\t') for l in open(tsv)]
    dem = [l.rstrip('\n') for l in open(names)]
    assert len(syms) == len(dem)
    lines = collections.Counter(); count = collections.Counter()
    total = 0
    for (s, n), d in zip(syms, dem):
        n = int(n); k = key(d)
        lines[k] += n; count[k] += 1; total += n
    print('total function lines:', total, 'distinct keys:', len(lines))
    def block(title, pred):
        sel = [k for k in lines if pred(k)]
        print(f'\n### {title}: keys {len(sel)} copies {sum(count[k] for k in sel)} lines {sum(lines[k] for k in sel)} ({100*sum(lines[k] for k in sel)/total:.1f}% of all)')
    for pat in ('with_resolved_semantics_inputs', 'with_semantics_inputs', 'with_semantics', 'with_resolved_semantics', 'semantic::tests::', '::tests::', 'semantic::tests::match_sites', 'driver::diagnostic::tests'):
        block(pat, lambda k, p=pat: p in k)
    print('\n### copies of the semantic test helpers (lines, copies, lines per copy, key)\n```text')
    for k in sorted(lines, key=lambda k: -lines[k]):
        if re.search(r'semantic::tests::(with_|assert_|check_)', k) and count[k] > 1:
            print(f'{lines[k]:9d} {count[k]:6d} {lines[k]//count[k]:7d}  {k[:140]}')
    print('```')
    def prefix(k, n):
        return '::'.join(k.replace('<', '').split('::')[:n])
    for depth in (3, 4):
        agg = collections.Counter(); cp = collections.Counter()
        for k in lines:
            agg[prefix(k, depth)] += lines[k]; cp[prefix(k, depth)] += count[k]
        print(f'\n### top 40 prefixes of depth {depth} by lines (lines, copies, prefix)\n```text')
        for k, l in agg.most_common(40):
            print(f'{l:9d} {cp[k]:6d}  {k[:130]}')
        print('```')
    print('\n### top 40 keys by copies (copies, lines, key)\n```text')
    for k in sorted(count, key=lambda k: -count[k])[:40]:
        print(f'{count[k]:6d} {lines[k]:9d}  {k[:140]}')
    print('```')

if sys.argv[1] == 'extract':
    extract(sys.argv[2], sys.argv[3])
else:
    report(sys.argv[2], sys.argv[3])
