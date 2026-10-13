"""Definition-level oracle for the std::float_text operations.

Computes each result directly from its definition with exact rational
arithmetic (no digit-generation algorithm), so it is independent of the
library under test. Standard library only; run with `python3 -I`.

    python3 -I reference.py check N SEED   # compare with Python's own
                                           # correctly rounded repr and %g
    python3 -I reference.py cases          # print the named writer cases
    python3 -I reference.py generate DIR [SCALE]
                                           # write the request and expected
                                           # reply files of every family

`generate` writes FAMILY.in and FAMILY.out into DIR: one request per line for
driver.wf and the reply line it must print. The line formats are those of
driver.wf's handle_line.
"""
import math
import os
import random
import re
import struct
import sys
from fractions import Fraction as F


def bits(d):
    return struct.unpack('<Q', struct.pack('<d', d))[0]


def decode(d):
    """Sign bit, integer significand m and exponent e with |d| = m * 2^e."""
    b = bits(d)
    biased = (b >> 52) & 2047
    fraction = b & ((1 << 52) - 1)
    if biased == 0:
        return b >> 63, fraction, -1074
    return b >> 63, fraction | (1 << 52), biased - 1075


def interval(d):
    """Exact value x of finite nonzero |d| and its rounding interval.

    Returns (x, low, high, inclusive): the reals v > 0 that round to |d| under
    round-to-nearest-ties-to-even are low < v < high, with both ends included
    exactly when the significand is even. The largest finite value's upper
    neighbour is the candidate 2^1024, as in OP-6.
    """
    _, m, e = decode(d)
    x = F(m) * F(2) ** e
    if m == (1 << 52) and e > -1074:
        low = x - F(2) ** (e - 2)
    else:
        low = x - F(2) ** (e - 1)
    high = x + F(2) ** (e - 1)
    return x, low, high, m % 2 == 0


def inside(v, low, high, inclusive):
    return low <= v <= high if inclusive else low < v < high


def floor_log10(v):
    k = len(str(v.numerator)) - len(str(v.denominator))
    while F(10) ** k > v:
        k -= 1
    while F(10) ** (k + 1) <= v:
        k += 1
    return k


def shortest(d):
    """(c, n, q) for finite nonzero d: least n, then nearest, then even c."""
    x, low, high, inclusive = interval(d)
    k = floor_log10(x)
    for n in range(1, 18):
        best = None
        for q in (k - n, k - n + 1, k - n + 2):
            p = F(10) ** q
            first = max(-((-low) // p), 10 ** (n - 1))
            last = min(high // p, 10 ** n - 1)
            for c in range(int(first), int(last) + 1):
                v = c * p
                if inside(v, low, high, inclusive):
                    key = (abs(v - x), c % 2)
                    if best is None or key < best[0]:
                        best = (key, c, q)
        if best:
            return best[1], n, best[2]
    raise AssertionError('no 17-digit candidate')


def significant(d, p):
    """(c, p, q) for finite nonzero d rounded to p significant digits."""
    x = interval(d)[0]
    q = floor_log10(x) - p + 1
    v = x / F(10) ** q
    c = int(v)
    rest = v - c
    if rest > F(1, 2) or (rest == F(1, 2) and c % 2 == 1):
        c += 1
    if c == 10 ** p:
        c //= 10
        q += 1
    return c, p, q


def general(negative, c, count, q, precision):
    """The proposed general layout: C's %.Pg layout of (-1)^negative c 10^q."""
    sign = '-' if negative else ''
    if c == 0:
        return sign + '0'
    digits = str(c).rjust(count, '0')
    lead = q + count - 1
    digits = digits.rstrip('0')
    if -4 <= lead < precision:
        if lead >= 0:
            whole = digits[:lead + 1].ljust(lead + 1, '0')
            part = digits[lead + 1:]
        else:
            whole = '0'
            part = '0' * (-lead - 1) + digits
        return sign + whole + ('.' + part if part else '')
    rest = '.' + digits[1:] if len(digits) > 1 else ''
    mark = '-' if lead < 0 else '+'
    return sign + digits[0] + rest + 'e' + mark + '%02d' % abs(lead)


def shortest_text(d):
    negative = bits(d) >> 63
    if d == 0:
        return general(negative, 0, 1, 0, 17)
    c, n, q = shortest(d)
    return general(negative, c, n, q, 17)


def significant_text(d, p):
    negative = bits(d) >> 63
    if d == 0:
        return general(negative, 0, 1, 0, p)
    c, n, q = significant(d, p)
    return general(negative, c, n, q, p)


def random_double(rng):
    while True:
        b = rng.getrandbits(64)
        if (b >> 52) & 2047 != 2047 and b & ~(1 << 63):
            return struct.unpack('<d', struct.pack('<Q', b))[0]


def check(count, seed):
    rng = random.Random(seed)
    failures = 0
    for _ in range(count):
        d = random_double(rng)
        if shortest_text(d) != general(*shortest_from_repr(d)):
            failures += 1
            print('shortest', d.hex())
        for p in (1, 6, 14, 17):
            if significant_text(d, p) != '%.*g' % (p, d):
                failures += 1
                print('significant', p, d.hex())
    print('inputs', count, 'failures', failures)
    return failures


def shortest_from_repr(d):
    """Python's repr digits (David Gay's shortest) as (negative, c, n, q, 17)."""
    text = repr(abs(d))
    mantissa, _, exponent = text.partition('e')
    whole, _, part = mantissa.partition('.')
    part = '' if part == '0' else part
    digits = (whole + part).lstrip('0')
    q = (int(exponent) if exponent else 0) - len(part)
    trimmed = digits.rstrip('0')
    q += len(digits) - len(trimmed)
    return bits(d) >> 63, int(trimmed), len(trimmed), q, 17


CASES = [
    ('0.1', 0.1), ('0.1 + 0.2', 0.1 + 0.2), ('1/3', 1 / 3), ('1e17', 1e17),
    ('1e16', 1e16), ('-0.0', -0.0), ('0.0', 0.0),
    ('smallest subnormal', 5e-324), ('largest subnormal', 2.225073858507201e-308),
    ('smallest normal', 2.0 ** -1022), ('largest finite', 1.7976931348623157e308),
    ('2^53 - 1', 2.0 ** 53 - 1), ('2^53', 2.0 ** 53), ('2^53 + 2', 2.0 ** 53 + 2),
    ('2^62', 2.0 ** 62), ('2^63', 2.0 ** 63), ('1e23', 1e23), ('9.66e21', 9.66e21),
    ('1e-5', 1e-5), ('1e-7', 1e-7), ('123456789012345', 123456789012345.0),
    ('99999999999999.5', 99999999999999.5),
]

DECIMAL = re.compile(
    r'([+-]?)(?:([0-9]+)(?:\.([0-9]*))?|\.([0-9]+))(?:[eE]([+-]?[0-9]+))?')

FRACTION_MASK = (1 << 52) - 1


def double_of(pattern):
    return struct.unpack('<d', struct.pack('<Q', pattern))[0]


def hex16(pattern):
    return '%016x' % pattern


def finite_pattern(pattern):
    return ((pattern >> 52) & 2047) != 2047


def digits_of(match):
    """The integer and fraction digits of a DECIMAL match, and its exponent."""
    whole = match.group(2) or ''
    fraction = match.group(3) or match.group(4) or ''
    exponent = int(match.group(5)) if match.group(5) else 0
    return whole, fraction, exponent


def exact_class(text):
    """'overflow', 'underflow' or 'ok' for a decimal text, from exact
    rationals: the rounded value is infinite exactly from the midpoint
    2^1024 - 2^970 up, and zero exactly up to the midpoint 2^-1075 (both
    ties go to the even candidate)."""
    whole, fraction, exponent = digits_of(DECIMAL.fullmatch(text))
    significand = int(whole + fraction)
    if significand == 0:
        return 'ok'
    scale = exponent - len(fraction)
    size = len(str(significand)) + scale
    if size - 1 >= 309:
        return 'overflow'
    if size <= -324:
        return 'underflow'
    value = F(significand) * F(10) ** scale
    if value >= F(2) ** 1024 - F(2) ** 970:
        return 'overflow'
    if value <= F(2) ** -1075:
        return 'underflow'
    return 'ok'


def read_expected(text):
    """The reply for a request R<text>: the rounded value from Python's
    correctly rounded float(), the class checked against exact rationals."""
    match = DECIMAL.fullmatch(text)
    if match is None:
        return 'syntax'
    whole, fraction, _ = digits_of(match)
    value = float(text)
    if not any(c != '0' for c in whole + fraction):
        kind = 'ok'
    elif math.isinf(value):
        kind = 'overflow'
    elif value == 0.0:
        kind = 'underflow'
    else:
        kind = 'ok'
    if kind != exact_class(text):
        raise SystemExit('the class of %r is %s by float() and %s by exact '
                         'rationals' % (text[:60], kind, exact_class(text)))
    return kind + ' ' + hex16(bits(value))


def nonfinite_text(pattern):
    sign = '-' if pattern >> 63 else ''
    return sign + ('inf' if (pattern & FRACTION_MASK) == 0 else 'nan')


def shortest_reply(pattern):
    if not finite_pattern(pattern):
        return nonfinite_text(pattern)
    return shortest_text(double_of(pattern))


def significant_reply(pattern, precision):
    if not finite_pattern(pattern):
        return nonfinite_text(pattern)
    return significant_text(double_of(pattern), precision)


def window_reply(pattern, capacity):
    text = shortest_reply(pattern)
    if len(text) <= capacity:
        return text
    return 'small %02d' % len(text)


def read_pair(text):
    return 'R' + text, read_expected(text)


def shortest_pair(pattern):
    return 'S' + hex16(pattern), shortest_reply(pattern)


def significant_pair(pattern, precision):
    return ('G%02d' % precision + hex16(pattern),
            significant_reply(pattern, precision))


def window_pair(pattern, capacity):
    return ('W%02d' % capacity + hex16(pattern),
            window_reply(pattern, capacity))


def midpoint_text(pattern):
    """The exact decimal text of the midpoint between a positive finite
    double and its successor."""
    _, significand, exponent = decode(double_of(pattern))
    numerator = 2 * significand + 1
    exponent -= 1
    if exponent >= 0:
        return str(numerator * 2 ** exponent)
    places = -exponent
    digits = str(numerator * 5 ** places).rjust(places + 1, '0')
    return digits[:-places] + '.' + digits[-places:]


def named_reads():
    texts = [
        '9007199254740993', '9007199254740995',
        '9007199254740993.' + '0' * 1000 + '1', '9007199254740993.' + '0' * 1000,
        '2.4703282292062327e-324', '2.4703282292062328e-324', '-1e-400',
        '1.7976931348623158e308', '1.7976931348623159e308', '-1e400',
        '0e99999999999999999999', '-0', '1.', '.5', '+.5e-3',
        '0.1000000000000000055511151231257827021181583404541015625',
        '', '.', '+', 'e5', '1e', '1e+', '1.2.3', ' 1', '1 ', '0x10', 'inf',
        'nan', '-', '+-1', '1e5.5', '1.e5', '.e5', '--1', '1e-', '1ee5',
        '0', '0.0', '-0.0', '+0', '1e22', '1e23', '3e22', '123456789012345',
        '1234567890123456789012', '5e-324', '1e-5', '+1', '1E5', '1e+5',
        '00012.5000', '0.000000000000000000000000000001e30',
        '123456789012345678901234567890e-20', '1e308', '1e309',
        '4.9406564584124654e-324', '2.2250738585072011e-308',
        '2.2250738585072014e-308', '0.' + '0' * 400 + '1e400',
    ]
    pairs = [read_pair(text) for text in texts]
    for name, d in CASES:
        if math.isinf(d) or d != d:
            continue
        pair = read_pair(shortest_text(d))
        if pair[1] != 'ok ' + hex16(bits(d)):
            raise SystemExit('the shortest text of %s does not read back' % name)
        pairs.append(pair)
    return pairs


def named_writes():
    patterns = [bits(d) for _, d in CASES]
    patterns += [0x7ff0000000000000, 0xfff0000000000000, 0x7ff8000000000000,
                 0xfff8000000000000, bits(-1.5), bits(-1e300), bits(1e300)]
    pairs = []
    for pattern in patterns:
        pairs.append(shortest_pair(pattern))
        if finite_pattern(pattern):
            pairs.append(significant_pair(pattern, 17))
            pairs.append(significant_pair(pattern, 14))
    for value, precision in [
            (2.5, 1), (0.125, 2), (0.375, 2), (0.5, 1), (1.5, 1), (9.5, 1),
            (99.5, 2), (0.000123456, 3), (123456789.0, 3), (1e-4, 1),
            (1e-5, 1), (0.0001234, 2), (100.0, 1), (100000.0, 6),
            (1000000.0, 6), (1e16, 17), (1e17, 17), (123456789012345678.0, 17),
            (0.0, 1), (-0.0, 17), (5e-324, 1), (1.7976931348623157e308, 1)]:
        pairs.append(significant_pair(bits(value), precision))
    for value, capacities in [
            (-2.2250738585072014e-308, (0, 1, 23, 24, 25, 60)),
            (1.0, (0, 1, 2, 3, 4)), (0.1, (2, 3, 4)), (-1e23, (4, 5, 6)),
            (0.0, (0, 1, 2)), (-0.0, (1, 2, 3))]:
        for capacity in capacities:
            pairs.append(window_pair(bits(value), capacity))
    return pairs


def sweep_random(count):
    rng = random.Random(1)
    pairs = []
    for _ in range(count):
        d = random_double(rng)
        pattern = bits(d)
        pairs.append(shortest_pair(pattern))
        for precision in (1, 6, 14, 17):
            pairs.append(significant_pair(pattern, precision))
        for text in (shortest_text(d), '%.17g' % d):
            pair = read_pair(text)
            if pair[1] != 'ok ' + hex16(pattern):
                raise SystemExit('%r does not read back to %s' % (text, d.hex()))
            pairs.append(pair)
    return pairs


def sweep_decimal_reads():
    return [read_pair('%de%d' % (m, e))
            for m in range(1, 1000) for e in range(-30, 31)]


def sweep_decimal_writes():
    pairs = []
    for m in range(1, 100):
        for e in range(-30, 31):
            pattern = bits(float('%de%d' % (m, e)))
            pairs.append(shortest_pair(pattern))
            pairs.append(significant_pair(pattern, 14))
    return pairs


def sweep_powers():
    pairs = []
    for k in range(-1074, 1024):
        pattern = bits(math.ldexp(1.0, k))
        for neighbour in (pattern - 1, pattern, pattern + 1):
            if finite_pattern(neighbour):
                pairs.append(shortest_pair(neighbour))
                pairs.append(significant_pair(neighbour, 17))
    return pairs


def sweep_midpoints(random_count):
    rng = random.Random(2)
    patterns = [
        0x0000000000000001, 0x0000000000000002, 0x0000000000000003,
        0x000fffffffffffff, 0x0010000000000000, 0x3ff0000000000000,
        0x3fefffffffffffff, 0x433fffffffffffff, 0x4340000000000000,
        0x4340000000000001, 0x7fefffffffffffff, bits(0.1), bits(1e23),
        bits(1e22), bits(5e-324 * 1024.0)]
    patterns += [bits(random_double(rng)) & ~(1 << 63)
                 for _ in range(random_count)]
    pairs = []
    for pattern in patterns:
        text = midpoint_text(pattern)
        point = '' if '.' in text else '.'
        pairs.append(read_pair(text))
        pairs.append(read_pair(text + point + '0' * 1000))
        pairs.append(read_pair(text + point + '0' * 1000 + '1'))
        pairs.append(read_pair('-' + text))
    return pairs


def generate(directory, scale):
    os.makedirs(directory, exist_ok=True)
    if hasattr(sys, 'set_int_max_str_digits'):
        sys.set_int_max_str_digits(0)
    families = [
        ('named_read', named_reads()),
        ('named_write', named_writes()),
        ('sweep_random', sweep_random(1000 * scale)),
        ('sweep_decimal_reads', sweep_decimal_reads()),
        ('sweep_decimal_writes', sweep_decimal_writes()),
        ('sweep_powers', sweep_powers()),
        ('sweep_midpoints', sweep_midpoints(100 * scale)),
    ]
    for name, pairs in families:
        with open(os.path.join(directory, name + '.in'), 'w',
                  newline='\n') as requests, \
                open(os.path.join(directory, name + '.out'), 'w',
                     newline='\n') as replies:
            for request, reply in pairs:
                if len(request) >= 4000 or '\n' in request:
                    raise SystemExit('a request of %s is too long' % name)
                requests.write(request + '\n')
                replies.write(reply + '\n')


if __name__ == '__main__':
    if hasattr(sys, 'set_int_max_str_digits'):
        sys.set_int_max_str_digits(0)
    if sys.argv[1] == 'check':
        sys.exit(1 if check(int(sys.argv[2]), int(sys.argv[3])) else 0)
    if sys.argv[1] == 'generate':
        generate(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 1)
        sys.exit(0)
    for name, d in CASES:
        print('|', name, '|', d.hex(), '|', shortest_text(d), '|',
              significant_text(d, 17), '|', significant_text(d, 14), '|')
