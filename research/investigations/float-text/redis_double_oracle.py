"""Asks a real redis-server how it writes doubles and compares it with the models.

Experiment E1 of DESIGN.md. Standard library only; raw RESP over a socket.
Run with `python3 -I`; the temporary workflow `float-text-redis.yml` builds
redis-server 7.2.16 and calls this driver against it.

    python3 -I redis_double_oracle.py --dry-run [--count N] [--out DIR]
        no server: prints the value sets, self-tests the RESP parser, and runs
        the whole comparison against a simulated server that answers with the
        design's model (every 97th answer perturbed, so the mismatch
        categories are exercised)
    python3 -I redis_double_oracle.py --port P --out DIR [--count N] [--seed S]
        queries the server at P

Every value goes through every path the design names:

    ZADD/ZSCORE, ZADD/ZRANGE WITHSCORES, ZINCRBY    (RESP2 and RESP3)
    EVAL returning {double=x}                         (RESP2 and RESP3)
    EVAL passing x to redis.call('SET') then GET      (RESP2 and RESP3)
    EVAL returning tostring(x)                        (RESP2)
    INCRBYFLOAT                                       (RESP2, informational)

and each path writes `<path>.tsv`: path, family, bits, input, Redis output,
reply type, exact shortest (reference.py), exact shortest in fpconv's layout,
the reconstruction's output, category. `read-cases.tsv` holds the texts the
design's reading table lists. `summary.txt` repeats what is printed.
"""
import argparse
import decimal
import math
import os
import random
import re
import socket
import struct
import sys
import time
from decimal import Decimal

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fpconv_reconstruction as recon  # noqa: E402
import reference  # noqa: E402

decimal.getcontext().prec = 2000
decimal.getcontext().Emax = decimal.MAX_EMAX
decimal.getcontext().Emin = decimal.MIN_EMIN

KEY = 'oracle:k'
MEMBER = 'm'
EXAMPLES = 20
LUA_DOUBLE = "return {double=tonumber(ARGV[1])}"
LUA_SET_GET = ("redis.call('SET', KEYS[1], tonumber(ARGV[1])) "
               "return redis.call('GET', KEYS[1])")
LUA_TOSTRING = "return tostring(tonumber(ARGV[1]))"
LIMIT = 2.0 ** 62


# ----------------------------------------------------------------- RESP

class Tagged(str):
    tag = 'str'


class Simple(Tagged):
    tag = 'simple'


class Bulk(Tagged):
    tag = 'bulk'


class Dbl(Tagged):
    tag = 'double'


class Big(Tagged):
    tag = 'bignum'


class Err(Tagged):
    tag = 'error'


class Reader:
    """Parses RESP2 and RESP3 replies from a function returning byte chunks."""

    def __init__(self, recv):
        self.recv = recv
        self.buf = b''
        self.pos = 0

    def fill(self):
        chunk = self.recv()
        if not chunk:
            raise EOFError('server closed the connection')
        self.buf = self.buf[self.pos:] + chunk
        self.pos = 0

    def line(self):
        while True:
            end = self.buf.find(b'\r\n', self.pos)
            if end >= 0:
                out = self.buf[self.pos:end]
                self.pos = end + 2
                return out
            self.fill()

    def exact(self, n):
        while len(self.buf) - self.pos < n:
            self.fill()
        out = self.buf[self.pos:self.pos + n]
        self.pos += n
        return out

    def reply(self):
        line = self.line()
        kind, body = line[:1], line[1:].decode('latin-1')
        if kind == b'+':
            return Simple(body)
        if kind == b'-':
            return Err(body)
        if kind == b':':
            return int(body)
        if kind in (b'$', b'!', b'='):
            n = int(body)
            if n < 0:
                return None
            data = self.exact(n + 2)[:-2].decode('latin-1')
            if kind == b'!':
                return Err(data)
            return Bulk(data[4:] if kind == b'=' else data)
        if kind in (b'*', b'~', b'>'):
            n = int(body)
            if n < 0:
                return None
            return [self.reply() for _ in range(n)]
        if kind == b'%':
            return [[self.reply(), self.reply()] for _ in range(int(body))]
        if kind == b'_':
            return None
        if kind == b'#':
            return body == 't'
        if kind == b',':
            return Dbl(body)
        if kind == b'(':
            return Big(body)
        raise ValueError('unknown reply type %r' % line[:40])


def encode(args):
    parts = [b'*%d\r\n' % len(args)]
    for arg in args:
        data = arg if isinstance(arg, bytes) else str(arg).encode('latin-1')
        parts.append(b'$%d\r\n' % len(data))
        parts.append(data)
        parts.append(b'\r\n')
    return b''.join(parts)


class Connection:
    def __init__(self, host, port, resp):
        self.sock = socket.create_connection((host, port), timeout=120)
        self.reader = Reader(lambda: self.sock.recv(1 << 16))
        if resp == 3:
            hello = self.call('HELLO', '3')
            assert isinstance(hello, list) and ['proto', 3] in hello, hello
        self.resp = resp

    def pipeline(self, commands):
        self.sock.sendall(b''.join(encode(c) for c in commands))
        return [self.reader.reply() for _ in commands]

    def call(self, *args):
        return self.pipeline([args])[0]


def parser_self_test():
    samples = [
        (b'+OK\r\n', 'OK', 'simple'), (b'-ERR x\r\n', 'ERR x', 'error'),
        (b':12\r\n', 12, None), (b'$3\r\nabc\r\n', 'abc', 'bulk'),
        (b'$0\r\n\r\n', '', 'bulk'), (b'$4\r\na\r\nb\r\n', 'a\r\nb', 'bulk'),
        (b'$-1\r\n', None, None), (b'_\r\n', None, None),
        (b',1e-7\r\n', '1e-7', 'double'), (b',-0\r\n', '-0', 'double'),
        (b'#t\r\n', True, None), (b'(123\r\n', '123', 'bignum'),
        (b'=8\r\ntxt:abcd\r\n', 'abcd', 'bulk'),
        (b'*2\r\n:1\r\n$1\r\na\r\n', [1, 'a'], None),
        (b'%1\r\n+k\r\n,2.5\r\n', [['k', '2.5']], None),
        (b'~1\r\n:1\r\n', [1], None),
    ]
    for wire, value, tag in samples:
        for step in (len(wire), 1, 3):
            chunks = [wire[i:i + step] for i in range(0, len(wire), step)]
            reader = Reader(lambda: chunks.pop(0) if chunks else b'')
            got = reader.reply()
            assert got == value, (wire, got)
            if tag:
                assert got.tag == tag, (wire, got.tag)
    assert encode(['SET', 'k', 'v']) == b'*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$1\r\nv\r\n'
    print('RESP parser self-test: %d samples, three chunkings each, ok' % len(samples))


# --------------------------------------------------------------- values

def cases():
    named = list(reference.CASES)
    named += [
        ('2^62 + 1024', math.nextafter(LIMIT, math.inf)),
        ('2^62 - 512', math.nextafter(LIMIT, 0.0)),
        ('1e22', 1e22), ('1e21', 1e21), ('1e-6', 1e-6), ('1.5e300', 1.5e300),
        ('123456.789', 123456.789), ('0.5', 0.5), ('100', 100.0), ('1', 1.0),
    ]
    out = list(named)
    out += [('-' + name, -v) for name, v in named if v != 0.0]
    return out


def integer_edge():
    out = []
    for base in (2.0 ** 53, 2.0 ** 61, LIMIT, 2.0 ** 63):
        for sign in (1.0, -1.0):
            b = sign * base
            out += [b, math.nextafter(b, math.inf), math.nextafter(b, -math.inf)]
    out += [-0.0, 0.5, -0.5, 1.0, -1.0, 1e18, 4.5e18, 4.7e18, 9.2e18,
            float(4611686018427387903), float(4611686018427387905)]
    seen, unique = set(), []
    for v in out:
        if reference.bits(v) not in seen:
            seen.add(reference.bits(v))
            unique.append(v)
    return unique


def layout_grid():
    out = []
    for m in (1, 5, 15, 123, 1234567, 12345678901234567):
        for e in range(-30, 31):
            out.append(float('%de%d' % (m, e)))
            if m in (15, 1234567):
                out.append(-float('%de%d' % (m, e)))
    return out


def value_sets(count, seed):
    def rng_for(name):
        return random.Random('%d-%s' % (seed, name))

    sets = [('case', [v for _, v in cases()])]
    rng = rng_for('random-bits')
    sets.append(('random-bits', [reference.random_double(rng) for _ in range(count)]))
    rng = rng_for('dec6')
    sets.append(('dec6', [float('%de%d' % (rng.randrange(1, 10 ** 6), rng.randrange(-16, 5)))
                          for _ in range(count)]))
    rng = rng_for('dec3-big')
    sets.append(('dec3-big', [float('%de%d' % (rng.randrange(1, 1000), rng.randrange(19, 23)))
                              for _ in range(count)]))
    rng = rng_for('uniform01')
    sets.append(('uniform01', [rng.random() for _ in range(count)]))
    sets.append(('layout-grid', layout_grid()))
    sets.append(('integer-edge', integer_edge()))
    return sets


class Value:
    """One double with everything the comparison needs, computed once."""

    def __init__(self, family, x):
        self.family = family
        self.x = x
        self.bits = reference.bits(x)
        self.hex = '%016x' % self.bits
        self.negative = bool(self.bits >> 63)
        self.text17 = '%.17g' % x
        self.exact_text = reference.shortest_text(x)
        if x == 0.0:
            self.exact = None
            self.exact_fpconv = ('-' if self.negative else '') + '0'
        else:
            self.exact = reference.shortest(x)
            self.exact_fpconv = ('-' if self.negative else '') + recon.fpconv_layout(
                str(self.exact[0]), self.exact[2], int(self.negative))
        self.d2string = recon.d2string(x)
        self.lua_arg = recon.lua_argument(x)
        self.dtoa = recon.fpconv_dtoa(x)
        self.tostring14 = '%.14g' % x
        self.integer_rule = recon.double2ll(x) is not None and x != 0.0
        self._incr = None
        self._exact_expansion = None

    @property
    def exact_expansion(self):
        if self._exact_expansion is None:
            self._exact_expansion = format(Decimal(self.x), 'f')
        return self._exact_expansion

    @property
    def incrbyfloat(self):
        """INCRBYFLOAT on a missing key: 0 + x as long double, "%.17Lf" trimmed."""
        if self._incr is None:
            if self.x == 0.0:
                self._incr = '0'
            else:
                q = Decimal(self.x).quantize(Decimal('1e-17'), rounding=decimal.ROUND_HALF_EVEN)
                text = format(q, 'f')
                if '.' in text:
                    text = text.rstrip('0').rstrip('.')
                self._incr = text
        return self._incr


def build_values(sets):
    values = []
    for family, xs in sets:
        seen = set()
        for x in xs:
            if reference.bits(x) in seen:
                continue
            seen.add(reference.bits(x))
            values.append(Value(family, x))
    return values


# ---------------------------------------------------------------- paths

class Path:
    def __init__(self, name, resp, model, build, note=''):
        self.name, self.resp, self.model, self.build, self.note = name, resp, model, build, note

    def expected(self, v):
        return {'d2string': v.d2string, 'lua_arg': v.lua_arg, 'tostring14': v.tostring14,
                'incrbyfloat': v.incrbyfloat}[self.model]

    def sent(self, v):
        return v.exact_expansion if self.model == 'incrbyfloat' else v.text17


def make_paths():
    def zscore(v):
        return [['DEL', KEY], ['ZADD', KEY, v.text17, MEMBER], ['ZSCORE', KEY, MEMBER]]

    def zrange(v):
        return [['DEL', KEY], ['ZADD', KEY, v.text17, MEMBER],
                ['ZRANGE', KEY, 0, -1, 'WITHSCORES']]

    def zincrby(v):
        return [['DEL', KEY], ['ZINCRBY', KEY, v.text17, MEMBER]]

    def lua(script):
        return lambda v: [['DEL', KEY], ['EVAL', script, 1, KEY, v.text17]]

    def incr(v):
        return [['DEL', KEY], ['INCRBYFLOAT', KEY, v.exact_expansion]]

    paths = []
    for resp in (2, 3):
        tag = '_resp%d' % resp
        paths += [
            Path('zscore' + tag, resp, 'd2string', zscore),
            Path('zrange_withscores' + tag, resp, 'd2string', zrange),
            Path('zincrby' + tag, resp, 'd2string', zincrby),
            Path('eval_double' + tag, resp, 'd2string', lua(LUA_DOUBLE)),
            Path('eval_set_get' + tag, resp, 'lua_arg', lua(LUA_SET_GET)),
        ]
    paths.append(Path('eval_tostring_resp2', 2, 'tostring14', lua(LUA_TOSTRING)))
    paths.append(Path('incrbyfloat_resp2', 2, 'incrbyfloat', incr,
                      'informational: long double, input sent as the exact decimal expansion'))
    return paths


def leaf(reply):
    while isinstance(reply, list) and reply:
        reply = reply[-1]
    return reply


def result_of(replies):
    """The text and reply type of a command group, or its first error."""
    for r in replies:
        if isinstance(r, Err):
            return 'ERR: ' + str(r)[:60], 'error'
    r = leaf(replies[-1])
    if r is None:
        return 'nil', 'nil'
    if isinstance(r, Tagged):
        return str(r), r.tag
    return str(r), type(r).__name__


def run_path(connections, path, values, batch=200):
    conn = connections[path.resp]
    results = []
    for i in range(0, len(values), batch):
        commands, spans = [], []
        for v in values[i:i + batch]:
            group = path.build(v)
            commands += group
            spans.append(len(group))
        replies = conn.pipeline(commands)
        pos = 0
        for n in spans:
            results.append(result_of(replies[pos:pos + n]))
            pos += n
    return results


def simulate_path(path, values):
    out = []
    for i, v in enumerate(values):
        text = v.text17 if i % 97 == 96 else path.expected(v)
        out.append((text, 'sim'))
    return out


# ------------------------------------------------------------- analysis

def number(text):
    try:
        return Decimal(text)
    except decimal.InvalidOperation:
        return None


def exact_value(v):
    c, _, q = v.exact
    return (-1 if v.negative else 1) * Decimal(c).scaleb(q)


def digit_count(d):
    return len(d.normalize().as_tuple().digits)


def classify(path, v, out):
    expected = path.expected(v)
    if out == expected:
        return 'match'
    if out.startswith('ERR'):
        return 'error reply'
    if out == 'nil':
        return 'nil reply'
    got, want = number(out), number(expected)
    if got is None:
        return 'not a number'
    if got == want:
        return 'same value, other text (layout)'
    return 'other value'


def alt_tags(v, out):
    tags = []
    if out == v.dtoa and out != v.d2string:
        tags.append('= bare fpconv_dtoa')
    if out == v.text17:
        tags.append('= printf %.17g')
    if out == v.exact_text:
        tags.append('= exact shortest in %g layout')
    return tags


class Report:
    def __init__(self):
        self.lines = []

    def __call__(self, text=''):
        print(text, flush=True)
        self.lines.append(text)


def short(text, n=48):
    return text if len(text) <= n else text[:n - 3] + '...'


def percent(a, b):
    return '%.2f%%' % (100.0 * a / b) if b else 'n/a'


def analyse_path(report, path, values, results, out_dir):
    cats, family_total, family_match = {}, {}, {}
    alts, grisu, grisu_family, roundtrip = {}, {'same': [], 'differ': []}, {}, []
    agree_recon = 0
    rows = []
    for v, (out, rtype) in zip(values, results):
        cat = classify(path, v, out)
        cats.setdefault(cat, []).append((v, out))
        family_total[v.family] = family_total.get(v.family, 0) + 1
        if cat == 'match':
            family_match[v.family] = family_match.get(v.family, 0) + 1
        if cat != 'match':
            for tag in alt_tags(v, out):
                alts.setdefault(tag, []).append((v, out))
        if path.model in ('d2string', 'lua_arg') and v.x != 0.0 and not v.integer_rule:
            got = number(out)
            if got is not None:
                same = got == exact_value(v)
                grisu['same' if same else 'differ'].append((v, out))
                fam = grisu_family.setdefault(v.family, [0, 0])
                fam[0 if same else 1] += 1
                if out == v.dtoa:
                    agree_recon += 1
        if path.model in ('d2string', 'lua_arg'):
            try:
                if v.x != 0.0 and float(out) != v.x:
                    roundtrip.append((v, out))
            except ValueError:
                pass
        rows.append((v, out, rtype, cat))

    total = len(values)
    report('')
    report('=== %s (RESP%d, model %s)%s' % (
        path.name, path.resp, path.model, ' -- ' + path.note if path.note else ''))
    report('values %d; match the model: %d (%s)' % (
        total, len(cats.get('match', [])), percent(len(cats.get('match', [])), total)))
    for cat in sorted(cats):
        if cat == 'match':
            continue
        report('  %-36s %d' % (cat, len(cats[cat])))
    for tag in sorted(alts):
        report('  of the mismatches, %-34s %d' % (tag, len(alts[tag])))
    report('  per family, matches/total: ' + ', '.join(
        '%s %d/%d' % (f, family_match.get(f, 0), family_total[f]) for f in family_total))
    types = {}
    for _, _, rtype, _ in rows:
        types[rtype] = types.get(rtype, 0) + 1
    report('  reply types: ' + ', '.join('%s %d' % kv for kv in sorted(types.items())))
    for cat in sorted(cats):
        if cat == 'match':
            continue
        report('  examples of "%s" (up to %d): family, bits, input -> Redis | model | exact shortest' % (
            cat, EXAMPLES))
        for v, out in cats[cat][:EXAMPLES]:
            report('    %-12s %s %s -> %s | %s | %s' % (
                v.family, v.hex, v.text17, short(out), short(path.expected(v)), v.exact_text))
    if path.model in ('d2string', 'lua_arg'):
        n = len(grisu['same']) + len(grisu['differ'])
        report('  digits against the exact shortest (nonzero, outside the integer rule): '
               'same %d, differ %d of %d (%s); equal to bare fpconv_dtoa reconstruction in %d' % (
                   len(grisu['same']), len(grisu['differ']), n,
                   percent(len(grisu['differ']), n), agree_recon))
        for f, (same, differ) in grisu_family.items():
            report('    %-14s differ %d of %d (%s)' % (f, differ, same + differ,
                                                       percent(differ, same + differ)))
        longer = [(v, o) for v, o in grisu['differ']
                  if digit_count(number(o)) > v.exact[1]]
        report('  of the differing, Redis has more digits than the exact shortest: %d' % len(longer))
        report('  examples where Redis digits differ from the exact shortest (up to %d):' % EXAMPLES)
        for v, out in grisu['differ'][:EXAMPLES]:
            report('    %-12s %s %s -> %s | exact %s | reconstruction %s' % (
                v.family, v.hex, v.text17, short(out), v.exact_text, v.dtoa))
        report('  outputs that do not read back to the input double: %d' % len(roundtrip))
        for v, out in roundtrip[:EXAMPLES]:
            report('    %-12s %s %s -> %s' % (v.family, v.hex, v.text17, short(out)))
    if out_dir:
        with open(os.path.join(out_dir, path.name + '.tsv'), 'w') as f:
            f.write('path\tfamily\tbits\tinput\tredis\treply_type\texact_shortest\t'
                    'exact_shortest_fpconv_layout\treconstruction\tcategory\n')
            for v, out, rtype, cat in rows:
                f.write('\t'.join([path.name, v.family, v.hex, v.text17, out, rtype, v.exact_text,
                                   v.exact_fpconv, path.expected(v), cat]) + '\n')
    return {id(v): out for v, out, _, _ in rows}


def cross_path(report, paths, values, outputs):
    base = paths[0]
    report('')
    report('=== agreement between paths (against %s)' % base.name)
    for path in paths[1:]:
        if path.model == 'incrbyfloat':
            continue
        differ = [v for v in values if outputs[path.name][id(v)] != outputs[base.name][id(v)]]
        if path.model != base.model and path.model != 'tostring14':
            kind = ' (different model, expected to differ where the integer rule or -0 applies)'
        else:
            kind = ''
        report('%-24s differs from %s on %d of %d values%s' % (
            path.name, base.name, len(differ), len(values), kind))
        for v in differ[:EXAMPLES]:
            report('    %-12s %s %s -> %s | %s -> %s' % (
                v.family, v.hex, v.text17, short(outputs[path.name][id(v)]),
                base.name, short(outputs[base.name][id(v)])))


def edge_report(report, paths, values, outputs):
    report('')
    report('=== integer rule edge (family integer-edge): output of each path')
    edge = [v for v in values if v.family == 'integer-edge']
    shown = [p for p in paths if p.model != 'incrbyfloat']
    for v in edge:
        report('%s  x = %s  design integer rule: %s' % (
            v.hex, v.text17, 'integer' if v.integer_rule else 'not integer'))
        by_text = {}
        for p in shown:
            by_text.setdefault(outputs[p.name][id(v)], []).append(p.name)
        for text, names in by_text.items():
            report('    %-28s %s' % (short(text, 28), ', '.join(names)))
    report('')
    report('=== integer rule: output of each path for +-2^62 and its upper neighbours')
    probes = [LIMIT, -LIMIT, math.nextafter(LIMIT, math.inf), -math.nextafter(LIMIT, math.inf)]
    probe_values = [next(v for v in edge if v.bits == reference.bits(x)) for x in probes]
    groups = {}
    for p in shown:
        row = tuple(outputs[p.name][id(v)] for v in probe_values)
        groups.setdefault(row, []).append(p.name)
    for row, names in groups.items():
        report('  paths: ' + ', '.join(names))
        for x, text in zip(probes, row):
            report('      %-26s -> %s' % (x.hex(), text))


# --------------------------------------------------------- read cases

DECIMAL_TEXT = re.compile(r'[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?\Z')

READ_TEXTS = [
    '9007199254740993', '9007199254740995',
    '9007199254740993.' + '0' * 1000 + '1', '9007199254740993.' + '0' * 1000,
    '2.4703282292062327e-324', '2.4703282292062328e-324', '-1e-400',
    '1.7976931348623158e308', '1.7976931348623159e308', '-1e400',
    '0e99999999999999999999', '1e99999999999999999999', '-0', '1.', '.5', '+.5e-3',
    '0.1000000000000000055511151231257827021181583404541015625',
    '1.00000000000000011102230246251565404236316680908203125',
    '1.00000000000000011102230246251565404236316680908203126',
    '2.2250738585072011e-308', '2.2250738585072012e-308', '4.9406564584124654e-324',
    '8.41e21', '1e23', '5e-324', '1e-7', '1E5', '00012', '1e+2',
    '', '.', '+', 'e5', '1e', '1e+', '1.2.3', ' 1', '1 ', '0x10', '0x1p-2', 'inf', '-inf',
    'nan', '1_0',
]


def read_expected(text):
    """Python's correctly rounded answer for a decimal text, as Redis would reply."""
    if not DECIMAL_TEXT.match(text):
        return None
    x = float(text)
    mantissa = re.split('[eE]', text)[0]
    nonzero = re.search('[1-9]', mantissa) is not None
    if math.isinf(x) or (x == 0.0 and nonzero):
        return 'ERR'
    return recon.d2string(x)


def run_read_cases(report, connections, dry, out_dir):
    report('')
    report('=== read cases: ZADD of the text then ZSCORE, and EVAL {double=tonumber(text)}')
    rows = []
    zadd_path = Path('zscore_resp2', 2, 'd2string', None)
    for text in READ_TEXTS:
        expected = read_expected(text)
        if dry:
            zs = expected if expected is not None else 'sim'
            lua = 'sim'
        else:
            conn = connections[2]
            r = conn.pipeline([['DEL', KEY], ['ZADD', KEY, text, MEMBER], ['ZSCORE', KEY, MEMBER]])
            zs = 'ERR' if isinstance(r[1], Err) else result_of(r[2:])[0]
            r = conn.pipeline([['EVAL', LUA_DOUBLE, 1, KEY, text]])
            lua = 'ERR' if isinstance(r[0], Err) else result_of(r)[0]
        if expected is None:
            status = 'observed'
        elif zs == expected:
            status = 'match'
        else:
            status = 'MISMATCH'
        rows.append((text, zs, lua, expected, status))
    count = {}
    for row in rows:
        count[row[4]] = count.get(row[4], 0) + 1
    report('texts %d; %s' % (len(rows), ', '.join('%s %d' % kv for kv in sorted(count.items()))))
    report('  (expected is the correctly rounded value through d2string, or ERR where the text '
           'overflows or underflows to zero; texts outside the decimal grammar are observed only)')
    for text, zs, lua, expected, status in rows:
        report('  %-9s %-22r ZSCORE %-24s Lua %-24s expected %s' % (
            status, short(text, 22), short(zs, 24), short(lua, 24), expected))
    if out_dir:
        with open(os.path.join(out_dir, 'read-cases.tsv'), 'w') as f:
            f.write('text\tzadd_zscore\teval_double\texpected\tstatus\n')
            for text, zs, lua, expected, status in rows:
                f.write('\t'.join([short(text, 60).replace('\t', ' '), zs, lua,
                                   str(expected), status]) + '\n')


# ----------------------------------------------------------------- main

def wait_for_server(host, port, seconds):
    deadline = time.time() + seconds
    while True:
        try:
            return Connection(host, port, 2)
        except OSError:
            if time.time() > deadline:
                raise
            time.sleep(0.5)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--dry-run', action='store_true')
    ap.add_argument('--host', default='127.0.0.1')
    ap.add_argument('--port', type=int, default=6379)
    ap.add_argument('--out')
    ap.add_argument('--count', type=int)
    ap.add_argument('--seed', type=int, default=20261012)
    ap.add_argument('--wait', type=float, default=30.0)
    args = ap.parse_args()
    count = args.count if args.count is not None else (200 if args.dry_run else 5000)
    if args.out:
        os.makedirs(args.out, exist_ok=True)
    report = Report()

    parser_self_test()
    sets = value_sets(count, args.seed)
    report('value sets (seed %d, count %d per random family):' % (args.seed, count))
    for family, xs in sets:
        report('  %-13s %5d values, first: %s' % (
            family, len(xs), ' '.join('%.17g' % x for x in xs[:4])))
    t0 = time.time()
    values = build_values(sets)
    report('model columns computed for %d values in %.1f s' % (len(values), time.time() - t0))

    paths = make_paths()
    connections = {}
    if not args.dry_run:
        first = wait_for_server(args.host, args.port, args.wait)
        connections[2] = first
        connections[3] = Connection(args.host, args.port, 3)
        info = first.call('INFO', 'server')
        fields = dict(line.split(':', 1) for line in str(info).splitlines()
                      if ':' in line and not line.startswith('#'))
        report('server: redis_version %s, git %s, os %s, arch_bits %s, gcc %s, mode %s' % (
            fields.get('redis_version'), fields.get('redis_git_sha1'), fields.get('os'),
            fields.get('arch_bits'), fields.get('gcc_version'), fields.get('redis_mode')))
        if fields.get('redis_version') != '7.2.16':
            report('refusing to compare: the server is not redis 7.2.16')
            return 2

    outputs = {}
    for path in paths:
        t0 = time.time()
        results = (simulate_path(path, values) if args.dry_run
                   else run_path(connections, path, values))
        outputs[path.name] = analyse_path(report, path, values, results, args.out)
        report('  (%d values in %.1f s)' % (len(values), time.time() - t0))
    cross_path(report, paths, values, outputs)
    edge_report(report, paths, values, outputs)
    run_read_cases(report, connections, args.dry_run, args.out)
    if args.dry_run:
        report('')
        report('dry run: the server was simulated, so these answers are the models\' own')
    if args.out:
        with open(os.path.join(args.out, 'summary.txt'), 'w') as f:
            f.write('\n'.join(report.lines) + '\n')
    return 0


if __name__ == '__main__':
    sys.exit(main())
