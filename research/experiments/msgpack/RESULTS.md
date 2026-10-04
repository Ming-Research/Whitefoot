# MessagePack experiment results

On 2026-10-04 the native package and adapter at local code milestone
`d4f839a711e0a7179c3ad1f7f4c0df0cfd0e5899` passed the checks below. Only `lib/msgpack/` and
`research/experiments/msgpack/` changed. No language specification, compiler,
Halo module, conformance case or gate wiring changed.

## Observations

| Check | Result |
| --- | --- |
| Library graph, direct compiler `--check-modules` | types, decode and encode accepted |
| Generated values compared byte for byte with Redis cmsgpack | 3000 passed |
| Total cmsgpack comparisons including fixed boundaries | 3064 passed |
| Additional full-family writer checks against independent wire expectations | 81 passed |
| Writer records decoded at a nonzero offset, with exact EOF checked | 3145 passed |
| Total independently checked decoder calls | 15064 passed |
| Format tags exercised, including reserved 0xc1 | 256 passed |
| Every nonempty proper prefix of each multi-byte tag fixture, at two offsets | 1274 passed |
| Selected prefixes of writer outputs, including 65,536-byte payloads | 6228 passed |
| Arbitrary byte sequences and offsets | 1000 passed |
| Deliberately wrong comparison, token, truncation and response-framing controls | 18 refused |

The writer tests preserve a pre-existing output byte while appending each
value. A composite operation appends an array containing a map whose value
is an array, then a boolean, using successive library calls. Its exact bytes
and each token's next offset are checked. Scalar, byte-payload, extension and
container-header round trips check decoded values/ranges/counts against a
separate Python wire oracle; container headers do not imply child validation.
Unsigned values through u64 maximum, signed values through i64 minimum,
non-minimal wire widths, both floating widths and NaN payload bits are covered.
Strings, binary and extensions cover empty payloads and the 31/32, 255/256,
65,535/65,536 boundaries; extension types cover signed endpoints. All five
fixext widths and ext8/16/32 decode, including missing type bytes.

`run.py` compares native writer output with both the unchanged C encoder and
independent `struct`-based expectations. Decoder expectations are computed
from MessagePack tags and input bytes, independently of Whitefoot code. The
runner checks process exit codes, response count, lengths and exact bytes.
The negative controls alter writer bytes, token fields and error fields,
and remove, truncate or append response frames; each must fail its check.
The checked-in reference wrapper includes Redis's source at build time; it
does not copy or reimplement the encoder. Python owns fixture generation and
binary-oracle comparisons, not language acceptance.

## Reproduction

Set `WF_COMPILER` to the supplied built compiler, `REDIS_LUA_SRC` to Redis
7.0.15's `deps/lua/src`, `LUA_ARCHIVE` to the bundled Lua `liblua.a`, and
`SCRATCH` to a directory outside this repository. Then, from the repository:

```sh
"$WF_COMPILER" --graph lib/msgpack/modules.wfg --check-modules
python3 research/experiments/msgpack/run.py \
  --compiler "$WF_COMPILER" --redis-src "$REDIS_LUA_SRC" \
  --lua-lib "$LUA_ARCHIVE" --scratch "$SCRATCH" --generated 3000
```

The runner directly invokes the compiler to build its standalone adapter and
`cc -O2` to build `reference.c`, linked against bundled Lua. It emits measured
counts and writes `results.json` into scratch. It uses no Cargo, run-check
wrapper, network, Halo or Lua imports in the Whitefoot graph. The reference
Lua binary reports Lua 5.1.5 and had no preloaded cmsgpack global, so the C
wrapper was necessary. All binaries and generated data stay outside the repo.

The supplied compiler's SHA-256 was
`58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11`. The reference `lua_cmsgpack.c` SHA-256 was
`0d2fd82fb47fc247a0e6f0a287f5429e81b7eb6735665e8d851bfb109961640d`.

The host was arm64 macOS, with Apple Clang 21.0.0. Packing uses binary numeric
values and raw bytes, with no libc text conversion or formatting. Consequently
this experiment does not select macOS text behavior over the specified glibc
Linux reference. It does not establish Linux execution of this package.

## Run sizing

A five-record native smoke test was run three times before scaling. The first
process launches took 0.346 seconds for Whitefoot and 0.203 seconds for C;
warm launches ranged from 0.0034 to 0.0036 and 0.0029 to 0.0032 seconds,
respectively. A 10-generated-value sample also exercised every tag and all
small-fixture truncations before the full run. These observations supported
one 3,000-generated-value run; they are run sizing, not a performance claim.
The final WF build took 2.858 seconds, separate from
writer execution (0.337 seconds) and decoder execution
(0.054 seconds). The C build took
0.137 seconds and reference execution
0.150 seconds. No performance benchmark was requested.

## Reference limitation and defined library behavior

`lua_cmsgpack.c` line 372 tests a Lua number with `IS_INT64_EQUIVALENT`, whose
macro casts to `int64_t` before checking equality. A finite value outside that
integer range, or a NaN, has undefined C conversion behavior. On this host,
exactly `2^63` produced `cf7fffffffffffffff`, while the defined Whitefoot
number policy produced float32 `ca5f000000`. Compiling the wrapper with
`-O1 -fsanitize=float-cast-overflow` and sending operation 5 with bits
`0x43e0000000000000` reported:

> runtime error: 9.22337e+18 is outside the range of representable values of type 'long long'

To reproduce that witness, build `reference.c` with the above flags and the
same include/library arguments, then send the following binary request plus
its stop record to the sanitizer executable:

```python
struct.pack('<BQQ', 5, 0x43e0000000000000, 0) + bytes(17)
```

The compatibility comparisons use finite values within the C int64 conversion
domain, plus infinities (which the reference excludes before casting).
Generated floating cases combine bounded f64 bit patterns and fractional
values exactly representable as f32. Out-of-range finite values and NaNs are
separate independent policy checks. The library guards the i64 conversion;
otherwise it chooses f32 on exact numeric round trip and f64 on failure.
It makes no byte-equality claim for reference inputs with undefined C casts.
Explicit `float32` and `float64` functions preserve caller-selected width.

## Remaining scope and uncertainty

The [API](API.md) is complete for this task. Halo's Redis `cmsgpack` binding,
Lua table classification and nesting policy, and extraction as a separate
project remain later work. There is no recursive object builder, streaming
input source, UTF-8 validator or timestamp interpretation in this pull API.

Payload lengths above u32 and total-length overflow are guarded in source but
not allocated as live runtime test inputs. No Linux/glibc run, repository-wide
`make check`, Cargo run, push or pull request was performed under this task's
explicit constraints. These research checks have an explicit runner and are
not wired into formal gates. The C undefined-conversion boundary above is the
only observed reference disagreement; it remains visible rather than being
claimed compatible.

## Review

The independent read-only reviewer (configured `gpt-6.1-sol`) inspected base
`7d6e73ea5ab7ae2996792d695b28d7df8f96dbc0` through code milestone
`d4f839a711e0a7179c3ad1f7f4c0df0cfd0e5899`, plus this results document.
Scope: A1–A4, D1–D4, C1–C5, applicable R/M/V items and experimental
machinery under T2–T6. Specification amendment, CI budget/correspondence,
approval-log, publication and merge checks were inapplicable to the explicit
scoped task. No green suites were rerun.

The review read every changed artifact, the reference encoders and relevant
specification/design ancestors, inspected recorded scratch results, computed
the compiler/reference hashes independently, and ran `git diff --check`.
It found no defects within scope. Design correspondence was checked against
the delegated API rationale; no forbidden tree files were changed. The final
review-record edit was self-checked and changed no implementation or test.

Found along the way: the reference's out-of-range float-to-int64 cast was
confirmed with a sanitizer witness and recorded above. The library uses a
defined conversion guard, and those inputs are tested independently. No
in-scope unresolved implementation defect was found.
