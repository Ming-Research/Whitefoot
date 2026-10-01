"""Insert a descent invariant into each sampled Snowghost loop and check it.

    WHITEFOOTC=<Snowghost's pinned whitefootc> python3 insert_descent.py RENDERER [ID...] [--reverse]

RENDERER is a copy of Snowghost 09d33ba's renderer/ directory, for example
from `git -C <snowghost> archive 09d33ba renderer | tar -x -C <dir>`; the
script edits one file at a time and restores it. sample.tsv beside this
script lists the loops, with their line ranges and the variable whose
descent is checked. The draw in draw() reproduces sample.tsv from
../loops.tsv. --reverse flips each direction, as the control.
"""
import csv, os, random, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))


def draw():
    rows = [r for r in csv.DictReader(open(os.path.join(HERE, '..', 'loops.tsv')), delimiter='\t')
            if r['repo'] == 'SG' and r['class'] in ('C1', 'C2', 'C3')
            and r['file'].startswith('renderer/') and '/oracle/' not in r['file']
            and '/tools/' not in r['file']]
    return sorted((r['id'] for r in random.Random(20260930).sample(rows, 20)), key=int)


def main():
    args = [a for a in sys.argv[1:] if a != '--reverse']
    reverse = '--reverse' in sys.argv
    renderer, only = os.path.abspath(args[0]), args[1:]
    compiler = os.environ['WHITEFOOTC']
    sample = {r['id']: r for r in csv.DictReader(open(os.path.join(HERE, 'sample.tsv')), delimiter='\t')}
    assert sorted(sample, key=int) == draw()
    for i in only or sorted(sample, key=int):
        r = sample[i]
        up = (r['direction'] == 'up') != reverse
        path = os.path.join(os.path.dirname(renderer), r['file'])
        orig = open(path).read()
        lines = orig.split('\n')
        start, end = int(r['line']) - 1, int(r['end']) - 1
        head = start
        while not lines[head].rstrip().endswith('{'):
            head += 1
        indent = ' ' * (len(lines[start]) - len(lines[start].lstrip()) + 2)
        var = r['measure_var']
        snap = f'{indent}let term_before_{i} = {var};'
        rel = f'term_before_{i} < {var}' if up else f'{var} < term_before_{i}'
        inv = f'{indent}invariant term_progress_{i}: {rel};'
        open(path, 'w').write('\n'.join(lines[:head + 1] + [snap] + lines[head + 1:end] + [inv] + lines[end:]))
        module = 'pkg::' + '::'.join(os.path.dirname(r['file']).split('/')[1:])
        done = subprocess.run([compiler, '--graph', 'modules.wfg', '--check-module', module],
                              cwd=renderer, capture_output=True, text=True)
        open(path, 'w').write(orig)
        text = done.stdout + done.stderr
        first = next((l for l in text.split('\n') if 'error[' in l), '')
        disp = next((l.strip() for l in text.split('\n') if 'disposition' in l), '')
        print('\t'.join([i, r['file'], r['line'], var, 'up' if up else 'down', str(done.returncode), first, disp]))


if __name__ == '__main__':
    main()
