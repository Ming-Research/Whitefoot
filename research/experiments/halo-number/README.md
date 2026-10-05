# Halo number comparison with PUC Lua

Compiler builds and checks have a persistent cache at
`${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/halo-number`.
`--cache DIR` overrides it; `--no-cache` disables caching. Cache paths must be
outside the repository and survive scratch-executable cleanup.
This runner reports program execution times, so native builds default to
`--full-lto`. The compiler rejects combining `--full-lto` with `--cache`;
`--incremental` selects the persistent cache instead. Cached runtime timings
have unvalidated differences from full LTO and are only sizing observations.
Use the default or explicit `--full-lto` for runtime performance measurements.
Reused binaries must have been built with the corresponding mode.

This explicitly invoked experiment checks `lib/halo/number` against the local
Redis 7.0.15 bundled PUC Lua 5.1.5. It is not wired into a compiler gate.
`compare.py` builds the standalone Whitefoot adapter in
`lib/halo/number/tests/modules.wfg`, runs the oracles, checks each result and
process exit code, and optionally writes [RESULTS.md](RESULTS.md).

From the repository root, using an existing v0.90 compiler and the reference
Lua executable with its adjacent headers and `liblua.a`:

```sh
whitefootc --cache "${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/halo-number" \
  --graph lib/halo/modules.wfg --check-modules
python3 research/experiments/halo-number/compare.py \
  --compiler /path/to/whitefootc --incremental --lua /path/to/lua/src/lua --samples 100
perl .github/run-check.pl halo-number-oracle \
  python3 research/experiments/halo-number/compare.py \
  --compiler /path/to/whitefootc --full-lto --lua /path/to/lua/src/lua \
  --samples 10000 --musl-source /path/to/musl/src/math \
  --results research/experiments/halo-number/RESULTS.md
```

On macOS, the current formatter follows Linux/glibc's signed-NaN spelling:
two fixed format cases produce `-nan` where the local Lua oracle produces `nan`.
The `--samples 1` cached and uncached full-LTO runs both exit 1 with identical
mismatch groups and examples. This is the existing platform difference
recorded under [Later change](RESULTS.md#later-change); it is not normalized
away by the runner. Linux reference qualification remains separate.

The small run sizes the batch first. The program builds and temporary files
stay beneath this experiment directory and are deleted after each run. Python
3 and a C11 compiler are required; neither the Whitefoot compiler nor Lua is
rebuilt. The source/test digest in the results covers every number `.wf` and
`.wfm` file and the standalone test graph; executable and archive digests
identify the tested tools. Durations size correctness runs and do not measure
VM performance. Rerun when number code or the corpus changes; retain results
as dated evidence when the implementation is superseded. The adapter and
oracle transport files serve this comparison and can be removed when this
library no longer needs PUC parity experiments.

## Oracle construction

[oracle.lua](oracle.lua) performs `tostring`, `tonumber`, `x ^ y`, `math.fmod`,
`math.floor` and `math.ceil` directly. Every input double starts as sixteen
hexadecimal IEEE 754 bits; string inputs are hexadecimal bytes, including
embedded NULs and non-ASCII bytes. No expected result comes from Halo's code.

The reference executable disables dynamic loading and binary chunk loading.
It runs the script with a Lua-only transport using `math.ldexp` to reconstruct
finite inputs exactly and `math.frexp` to extract their bits. This transport
preserves finite values, signed zeros and infinities, and supplies NaNs of the
requested sign; it cannot preserve or inspect NaN payloads or signalling bits.

For those bits, [bits.c](bits.c) is linked against the unchanged, adjacent
reference `liblua.a`. It creates a Lua state, installs two C helpers that only
copy doubles to/from `uint64_t` using `memcpy`, and runs the same script. All
number operations still execute inside the reference Lua library. The script
also runs in the supplied reference executable over the entire corpus. The
comparison requires agreement between the archive host and executable on
every format byte, parse verdict and finite/infinite result bit, and every
NaN classification. Exact NaN result bits come from the archive host. This
cross-check does not independently establish signalling-NaN payload behavior
in the executable; its payload-preserving transport is unavailable.

The standalone Whitefoot adapter accepts a 17-byte header: one opcode and
two little-endian `u64` words. Opcodes 1, 3, 4, 5 and 6 select format, pow,
fmod, floor and ceil; the words hold argument bits. Opcode 2 uses its first
word as a byte length (at most 2048), followed by that many string bytes.
Opcode 0 terminates normally. Output is one line per input: number text,
sixteen hexadecimal result bits, or `nil` for a refused string. Reads and
writes handle partial transfers; an incomplete record or I/O failure exits
nonzero. The formatter receives its minimum permitted 32-byte buffer.

## Corpus and interpretation

The fixed seed is in `compare.py`. At the default scale the corpus includes:

- 10,000 random double bit patterns, plus signed zero, signed infinities,
  quiet/signalling NaNs, subnormal boundaries, integers, powers of two and
  ten, neighboring doubles, exact decimal halfway values and notation edges;
- tricky strings including ASCII whitespace, empty inputs, signs, exponents,
  hexadecimal integers/fractions, range errors, NaN payloads, incomplete forms,
  suffix garbage, C-string termination, and long decimal ties with sticky
  digits, plus independently generated decimal and hexadecimal strings;
- 10,000 power pairs divided between arbitrary bit patterns, wide positive
  magnitudes, ordinary finite arguments, bases close to one with large
  exponents, and negative bases with integer exponents, plus a special-value
  cross product;
- supplementary exact-bit comparisons for fmod, floor and ceil.

Format and parse mismatches make the experiment fail, as do supplementary
wrapper mismatches. Power differences are reported without changing the
oracle or expected result. ULP distance is the difference of monotonically
ordered IEEE encodings for finite results; negative and positive zero are
adjacent under this metric. NaN differences and other nonfinite differences
are counted separately and have no ULP distance. Bit equality is the primary
comparison for every arithmetic operation, including NaN payloads and zero
signs. Comparator controls deliberately change a format byte, parse verdict,
power bit and row count, and verify that each alteration is detected.

## Implementation lineage and platform limits

The decimal conversion and formatter adapt Firn's
`apps/firn/scores/{decimal,read,write}.wf`; formatting always rounds the exact
binary value to 14 significant digits, removing Firn's integer shortcut.
`fmod`, floor and ceil use the specification's `frem`, `ffloor` and `fceil`.

The power algorithm and tables port musl's Arm `pow.c`, `pow_data.c` and
`exp_data.c` from the local Emscripten SDK musl tree (SDK 3.1.12). The FMA log
path and compensated exponential path retain the original operation order;
all arithmetic is explicit Whitefoot `.strict`. The original stated
worst-case error is 0.54 ULP, not a promise of bit equality with another C
library. Host-observed NaN propagation (sign, payload and argument priority)
follows the macOS oracle explicitly. Floating exception flags and non-nearest rounding modes are outside Whitefoot's
floating-operation interface. The optional `--musl-source` comparison compiles
the original local C FMA algorithm and tables with contraction disabled. It changes only dependency
includes and the exported function name, and supplies bit helpers and
round-to-nearest exception-result adapters. It uses `WANT_SNAN=0`, as the local
musl header does, and disables rounding-mode branches that have the same
returned bits in round-to-nearest. The measured port agrees on every non-NaN
result. NaN propagation differs deliberately to match the macOS C oracle.
This separates algorithm differences between musl and the host C library from
translation defects. The sampled comparison is evidence, not a proof for all
binary64 argument pairs.

The library targets the macOS C locale of this task's oracle. In particular,
macOS printf writes both NaN signs as `nan`, while strtod preserves the sign
and a numeric payload. Its payload grammar uses decimal, leading-zero octal
or lowercase `0x` hexadecimal; other payload text still gives a NaN with a
zero payload. C99 hexadecimal fractions are read by strtod before Lua's
strtoul fallback would be reached. All other trailing nonspace bytes are
refused. Behavior on another libc or locale has not been established.

## Finding while adapting Firn

Firn's existing `score_text` in `apps/firn/scores/write.wf` prepends the sign
before its NaN branch, so a negative NaN would produce `-nan`. The macOS
printf oracle here produces `nan` for that bit pattern. Firn's sorted-set
reader rejects NaN, which limits the immediate impact, but `reply_score` has
an unrestricted f64 parameter and documents printf formatting. This existing
writer discrepancy is outside this task's allowed files and was left
unchanged; reopen if that writer receives NaNs or is reused for arbitrary
floating values. Halo's formatter handles this platform behavior explicitly.

## musl license

Copyright (c) 2018, Arm Limited. SPDX-License-Identifier: MIT.

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
