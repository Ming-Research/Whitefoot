# JSON package results

The standalone package and native experiment passed on 2026-10-04.
The tested source revision was `791fb93b45b856e9196648ee33f99a02c563564b`. The later report/review
commit does not replace that source identity.

## Results

| Observation | Result |
| --- | --- |
| Package modules checked directly | number, text, decode, encode accepted |
| Native adapter build | exit 0 |
| Repository `make static` | exit 0 |
| Generated JSON documents | 4000 passed, seed 8259 |
| Total document round trips | 4070 passed |
| Error code, byte offset and message | 86 passed |
| Direct writer UTF-8/control strings | 3 passed |
| Writer state/refusal/recovery sequence | passed, zero failing observations |
| Decimal conversions against exact rational rounding | 4080 passed |
| Deliberately corrupted oracle results | 12 detected |
| Scanner/string helper boundary records | 9 passed |
| Total native records | 8249 passed |

Document comparisons independently reconstruct the decoder events, compare them
with Python's JSON value, and parse the writer's re-encoding with Python. Integer
tokens use binary64 in the comparison, consistent with this API; source numeric
text is retained by the writer. Number ranges match independently located token occurrences in order; conversion
bits match the integer rational oracle. Both duplicate key events were retained. Error results
include repeat-call checks that the original error persists; End is also repeated.

Fixed observations include every source diagnostic class (1..14), both writer
slash modes, every control escape, NUL, valid surrogate pairs, refused lone
surrogates, raw UTF-8 scalar boundaries and invalid encodings, incomplete tokens,
comments/BOM refusal, trailing comma/input refusal, root scalars at depth zero,
arrays at depth 4096 and one over their limit, 512 nested objects and mixed
nesting. A 32,000-byte string exercises scratch/output growth. Writer checks
cover classes 16..19, caller prefixes, mismatched ends, missing values, invalid
text without mutation, continued writing after refusal and completion. Public scanner and quoted-string
helpers also test starts at/past EOF, u64-max and nonzero valid positions.

The decimal fixtures include normal/subnormal boundaries, signed zero,
overflow/underflow, huge exponent text, exact dyadic halfway values and values
on both sides, and a nonzero decimal digit past the 800 retained digits that
breaks a tie. The exact rational expectation additionally agrees with Python's
binary64 conversion on every conversion fixture.

## Commands and environment

```sh
whitefootc --graph lib/json/modules.wfg --check-modules
python3 -B research/experiments/json/run.py \
  --compiler /path/to/whitefootc --binary /tmp/json-check \
  --build --samples 4000 --report /tmp/json-results.json
```

The actual run used the supplied compiler executable directly, with no Cargo,
network access, compiler check wrapper, push or PR. Repository static checks use
the standard `make static` command. Host: Darwin, Python
3.14.7. Compiler SHA-256:
`58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11`.
Native adapter SHA-256:
`072961cac332ac858dbca034415c72ee7cb7f107f353f59358157acfd9f7460e`.
Package source SHA-256:
`43b446f79ca210e4313149a4a1744e5e961130aa2352c6bc283cac96ba38fbf1` (the runner hashes sorted package-relative paths,
NUL separators and file contents).

The small 20-document sample plus boundary fixtures passed before scaling;
three native execution times were 0.009651, 0.012102 and 0.010159 seconds.
After the review fixes, the expanded small sample also passed (native execution
0.403934 seconds). The full build took 3.353448 seconds; native batch execution took
0.618750 seconds. These size the correctness run and are
not a compiler or library performance comparison.

## Limits and remaining work

No specification or conformance evidence changed. The full repository gate,
Halo integration, Linux execution and a direct glibc strtod comparison were not
run. This implementation calls no libc; its integer rational oracle checks the
nearest/ties-to-even conversion contract that C-locale glibc strtod under
FE_TONEAREST supplies. The compiler executable is identified by its bytes;
its exact build-commit provenance was not independently established.

Capacity-ceiling error 20 is not dynamically exercised: reaching u64-max buffer
capacity cannot be allocated on this host. Adapter consistency code 15 is not a
source error and should not occur on valid API use. Physical allocator exhaustion
retains Whitefoot's allocation behavior. The experiment adapter bounds each
record to 65,536 bytes; the library has no such bound. Generated documents and
finite cases do not establish an exhaustive proof of JSON/Unicode correctness.

The package/API shapes and Unicode choices are documented in module interfaces
and [README.md](README.md). Design-tree edits and maintained TODO edits were
outside the owner's explicit two-directory scope. All code has a general path;
no Lua, Halo or Redis operation is called. Later Halo cjson bindings remain a
separate task.

## Review and findings fixed

A separate read-only reviewer checked the complete change from base
`7d6e73ea5ab7ae2996792d695b28d7df8f96dbc0` through the tested revision, using
checklist groups A, D, C, R, M, V and the applicable experiment-integrity items
in T. No specification, conformance, compiler, gate wiring or publication changed.
The reviewer inspected interfaces, phase machines, Unicode paths, copied decimal
conversion, the oracle and package structure, then re-reviewed the fixes. The
three findings were fixed and the limited re-review found none within scope:

- Public scanner and quoted-string helper errors now clamp past-EOF starting
  offsets to source length, with tests at EOF, one past it and u64-max.
- Numeric source ranges now match independently located tokens in order. A
  repeated-token mutation control rejects assigning both numbers in `[1,1]`
  the first token's range; that incorrect result had passed the initial oracle.
- The copied decimal formatter helpers with no callers or exports were removed.

The repository static check additionally found literal Han characters in a Python
fixture. Their equivalent Unicode escape spellings retain the generated text
and satisfy the English-artifact check. No out-of-scope files were changed.
