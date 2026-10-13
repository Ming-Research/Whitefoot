"""Definition-level reference for the proposed std::float_text operations.

Computes each proposed result directly from its definition in DESIGN.md with
exact rational arithmetic (no digit-generation algorithm), so it can serve as
an independent oracle. Standard library only; run with `python3 -I`.

    python3 -I reference.py check N SEED   # compare with Python's own
                                           # correctly rounded repr and %g
    python3 -I reference.py cases          # print the case table of DESIGN.md
"""
import random
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

if __name__ == '__main__':
    if sys.argv[1] == 'check':
        sys.exit(1 if check(int(sys.argv[2]), int(sys.argv[3])) else 0)
    for name, d in CASES:
        print('|', name, '|', d.hex(), '|', shortest_text(d), '|',
              significant_text(d, 17), '|', significant_text(d, 14), '|')
