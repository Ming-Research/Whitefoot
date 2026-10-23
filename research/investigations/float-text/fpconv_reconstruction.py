"""Reconstruction of Redis 7.2's double-to-text path, written from memory.

This models deps/fpconv/fpconv_dtoa.c (night-shift's Grisu2 port) and the
callers d2string() and the Lua argument conversion in script_lua.c, as
recalled without the source at hand. It is exploratory evidence only: the
float-text DESIGN.md's first experiment replaces it with the real C code built
from the Redis 7.2.16 release. Standard library only; run with `python3 -I`.

    python3 -I fpconv_reconstruction.py compare N SEED

compares Grisu2's digits with the exact shortest digits of reference.py, both
in fpconv's layout, on four input families.
"""
import os
import random
import struct
import sys
from fractions import Fraction as F

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import reference  # noqa: E402

M64 = (1 << 64) - 1
FRACMASK = 0x000FFFFFFFFFFFFF
EXPMASK = 0x7FF0000000000000
HIDDEN = 0x0010000000000000


def cached_power(k):
    """10^k as a 64-bit significand rounded to nearest and a binary exponent."""
    v = F(10) ** k
    e = v.numerator.bit_length() - v.denominator.bit_length() - 63
    while v / F(2) ** e >= 2 ** 64:
        e += 1
    while v / F(2) ** e < 2 ** 63:
        e -= 1
    scaled = v / F(2) ** e
    f = int(scaled)
    if scaled - f >= F(1, 2):
        f += 1
    if f == 1 << 64:
        f >>= 1
        e += 1
    return f, e


POWERS = [cached_power(-348 + 8 * i) for i in range(87)]
TENS = [10 ** (19 - i) for i in range(20)]


def build_fp(d):
    b = reference.bits(d)
    frac = b & FRACMASK
    exp = (b & EXPMASK) >> 52
    if exp:
        return [frac + HIDDEN, exp - 1075]
    return [frac, -1074]


def normalize(fp):
    while fp[0] & HIDDEN == 0:
        fp[0] = (fp[0] << 1) & M64
        fp[1] -= 1
    fp[0] = (fp[0] << 11) & M64
    fp[1] -= 11


def boundaries(fp):
    upper = [((fp[0] << 1) + 1) & M64, fp[1] - 1]
    while upper[0] & (HIDDEN << 1) == 0:
        upper[0] = (upper[0] << 1) & M64
        upper[1] -= 1
    upper = [(upper[0] << 10) & M64, upper[1] - 10]
    shift = 2 if fp[0] == HIDDEN else 1
    lower = [((fp[0] << shift) - 1) & M64, fp[1] - shift]
    lower = [(lower[0] << (lower[1] - upper[1])) & M64, upper[1]]
    return lower, upper


def find_cached(exp):
    approx = int(-(exp + 87) * 0.30102999566398114)
    index = int((approx + 348) / 8)
    while True:
        current = exp + POWERS[index][1] + 64
        if current < -60:
            index += 1
        elif current > -32:
            index -= 1
        else:
            return POWERS[index], -348 + index * 8


def multiply(a, b):
    lo = 0xFFFFFFFF
    ah_bl = (a[0] >> 32) * (b[0] & lo)
    al_bh = (a[0] & lo) * (b[0] >> 32)
    al_bl = (a[0] & lo) * (b[0] & lo)
    ah_bh = (a[0] >> 32) * (b[0] >> 32)
    tmp = (ah_bl & lo) + (al_bh & lo) + (al_bl >> 32) + (1 << 31)
    return [(ah_bh + (ah_bl >> 32) + (al_bh >> 32) + (tmp >> 32)) & M64,
            a[1] + b[1] + 64]


def round_digit(digits, delta, rem, kappa, frac):
    while rem < frac and delta - rem >= kappa and (
            rem + kappa < frac or frac - rem > rem + kappa - frac):
        digits[-1] -= 1
        rem += kappa


def generate(fp, upper, lower, k):
    wfrac = (upper[0] - fp[0]) & M64
    delta = (upper[0] - lower[0]) & M64
    shift = -upper[1]
    one = 1 << shift
    part1 = upper[0] >> shift
    part2 = upper[0] & (one - 1)
    digits = []
    kappa = 10
    for div in TENS[10:20]:
        digit = part1 // div
        if digit or digits:
            digits.append(digit)
        part1 -= digit * div
        kappa -= 1
        rest = (part1 << shift) + part2
        if rest <= delta:
            round_digit(digits, delta, rest, div << shift, wfrac)
            return digits, k + kappa
    unit = 18
    while True:
        part2 = (part2 * 10) & M64
        delta = (delta * 10) & M64
        kappa -= 1
        digit = part2 >> shift
        if digit or digits:
            digits.append(digit)
        part2 &= one - 1
        if part2 < delta:
            round_digit(digits, delta, part2, one, (wfrac * TENS[unit]) & M64)
            return digits, k + kappa
        unit -= 1


def grisu2(d):
    """Grisu2 digits and K, with |d| close to digits * 10^K."""
    w = build_fp(d)
    lower, upper = boundaries(w)
    normalize(w)
    power, k = find_cached(upper[1])
    w = multiply(w, power)
    upper = multiply(upper, power)
    lower = multiply(lower, power)
    lower[0] += 1
    upper[0] -= 1
    digits, big_k = generate(w, upper, lower, -k)
    return ''.join(map(str, digits)), big_k


def fpconv_layout(digits, k, negative):
    """fpconv's emit_digits for digits * 10^k."""
    count = len(digits)
    exp = abs(k + count - 1)
    if k >= 0 and exp < count + 7:
        return digits + '0' * k
    if k < 0 and (k > -7 or exp < 4):
        offset = count + k
        if offset <= 0:
            return '0.' + '0' * -offset + digits
        return digits[:offset] + '.' + digits[offset:]
    count = min(count, 18 - negative)
    out = digits[0] + ('.' + digits[1:count] if count > 1 else '')
    out += 'e' + ('-' if k + count - 1 < 0 else '+')
    hundreds = exp // 100 if exp > 99 else 0
    if hundreds:
        out += str(hundreds)
        exp -= hundreds * 100
    if exp > 9:
        out += str(exp // 10)
        exp %= 10
    elif hundreds:
        out += '0'
    return out + str(exp)


def fpconv_dtoa(d):
    negative = reference.bits(d) >> 63
    sign = '-' if negative else ''
    magnitude = abs(d)
    if magnitude == 0:
        return sign + '0'
    if magnitude != magnitude:
        return sign + 'nan'
    if magnitude == float('inf'):
        return sign + 'inf'
    digits, k = grisu2(magnitude)
    return sign + fpconv_layout(digits, k, negative)


def double2ll(d):
    """Redis util.h double2ll: integral d within [-2^62, 2^62] as an integer."""
    limit = float(2 ** 62)
    if d != d or d < -limit or d > limit:
        return None
    whole = int(d)
    return whole if whole == d else None


def d2string(d):
    """Redis 7.2 util.c d2string, used for double replies."""
    if d != d:
        return 'nan'
    if d in (float('inf'), float('-inf')):
        return '-inf' if d < 0 else 'inf'
    if d == 0:
        return '-0' if reference.bits(d) >> 63 else '0'
    whole = double2ll(d)
    return str(whole) if whole is not None else fpconv_dtoa(d)


def lua_argument(d):
    """Redis 7.2 script_lua.c conversion of a Lua number argument."""
    whole = double2ll(d)
    return str(whole) if whole is not None else fpconv_dtoa(d)


def exact_in_fpconv_layout(d):
    c, _, q = reference.shortest(d)
    return fpconv_layout(str(c), q, 0)


def compare(count, seed):
    rng = random.Random(seed)
    families = [
        ('random bit patterns', lambda: reference.random_double(rng)),
        ('6-digit decimals, 1e-16 to 1e10',
         lambda: float('%de%d' % (rng.randrange(1, 10 ** 6), rng.randrange(-16, 5)))),
        ('3-digit decimals, 1e19 to 1e25',
         lambda: float('%de%d' % (rng.randrange(1, 1000), rng.randrange(19, 23)))),
        ('uniform in [0, 1)', rng.random),
    ]
    for label, draw in families:
        differ = 0
        for _ in range(count):
            d = draw()
            if fpconv_layout(*grisu2(d), 0) != exact_in_fpconv_layout(d):
                differ += 1
        print('%s: %d of %d differ' % (label, differ, count))


if __name__ == '__main__':
    compare(int(sys.argv[2]), int(sys.argv[3]))
