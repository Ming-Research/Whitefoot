# Standalone JSON package experiment

The package graph is [lib/json/modules.wfg](../../../lib/json/modules.wfg).
Its four modules depend only on each other. The native adapter in `check/`
imports the package plus standard process/IO modules; the Python runner is an
independent oracle, explicitly invoked here rather than wired into a gate.
These fixtures stay here while the package API is experimental; promote useful
cases into maintained library tests when the package acquires that test owner.

```sh
python3 -B research/experiments/json/run.py \
  --compiler /path/to/whitefootc --binary /tmp/json-check \
  --build --samples 20
python3 -B research/experiments/json/run.py \
  --compiler /path/to/whitefootc --binary /tmp/json-check \
  --samples 4000 --report /tmp/json-results.json
```

The first command directly compiles the native executable and runs a small
sample. The second reuses that executable and checks 4,000 generated documents
and 4,000 generated number texts, with deterministic seed 8259. Alternate seeds
are available with `--seed`. A failure exits nonzero; expected results are never
regenerated from the implementation. Scratch binaries and machine-readable
reports belong outside the repository. [RESULTS.md](RESULTS.md) records the run.

## API and representation

Exact signatures and behavior live in each `module.wfm`:

- `json::decode`: `new(max_depth)` and `next(source, decoder, decoded)` yield
  object/array begin and end, key, string, number, true, false, null and End.
  Key/String bytes replace a caller-owned scratch `Box<Slots<u8>>`; Number
  includes a half-open source range and correctly rounded `f64`. The same
  immutable source slice must be supplied throughout. Readonly fields expose
  the cursor and state for inspection; callers use the module's operations.
- `json::encode`: a Writer controls automatic commas and object key/value
  order while each operation appends to the same caller-owned byte buffer.
  Number accepts validated caller-formatted text. `finish` checks that a single
  value is complete. Existing buffer prefixes are preserved. Invalid string
  or number text and ordinary sequencing errors leave output and state intact.
- `json::number`: a JSON syntax scanner and a complete-text binary64 converter.
  Decimal conversion copies the decimal-only Simple Decimal Conversion code
  from `lib/halo/number`, retaining 800 significant digits plus a sticky bit;
  there is no import or runtime dependency on Halo. The independent exact
  rational oracle tests round-to-nearest/ties-to-even, including normal,
  subnormal, signed-zero and overflow boundaries and a tie broken by a digit
  beyond the retained precision. There is no libc call or locale dependence.
- `json::text`: shared UTF-8 validation, JSON escape decoding, reusable buffers
  and owned diagnostics.

The pull interface retains only open-container phases, so nesting uses heap
storage rather than recursive calls. String scratch can be reused between
calls. A depth limit counts open objects and arrays; zero permits scalar roots.
Writer nesting has no separately imposed depth limit. State is for one document;
construct a new state to begin another. On a decoder error, repeated calls
return the original diagnostic; successful End repeats too.

Invalid UTF-8 and lone surrogates are rejected. The byte/string API represents
Unicode scalar strings as UTF-8; replacement would lose the supplied string,
while preserving lone UTF-16 code units would require a different representation.
Valid surrogate pairs decode to their scalar's UTF-8 sequence. Duplicate object
keys are emitted in order, leaving duplicate policy to the caller. Numeric
source text is preserved by the writer, so a valid JSON number that overflows
binary64 remains valid JSON text even though its conversion is infinity.

## Independent observations

The runner reconstructs values from the decoder event stream and compares them
with `python3`'s `json.loads`; it separately parses the writer's output with
Python. Numeric events are checked against their original source range and an
integer rational-rounding oracle. The Python expectation uses binary64 for
integer tokens too, matching the package's numeric API while distinguishing
booleans and signed zeros. Writer numeric output retains the original token.
A duplicate-key fixture additionally checks both key events.

Fixed cases cover each source diagnostic class and its byte offset/message,
all control escapes, UTF-8 scalar boundaries, bad UTF-8, surrogate pairs and
lone surrogates, number syntax, slash modes, caller prefixes, writer misuse and
reuse after refusal. Nesting is checked at and one beyond the limit, including
4,096 arrays and 512 objects. A 32,000-byte string exercises buffer growth and
scratch reuse. Exact decimal halfway cases are generated from dyadic rational
values, without a float formatter. Mutation controls intentionally corrupt
writer output, event text, numeric ranges/bits, event completeness, error codes,
offsets and messages; each must be detected.

Source diagnostic codes 1..14 and writer sequencing codes 16..19 have executable
cases. Code 15 is reserved for adapter consistency failures. Code 20 reports
the unallocatable u64 buffer-capacity ceiling; that resource boundary cannot be
reached by this experiment. Physical allocation exhaustion remains the
Whitefoot allocator's behavior. The adapter's 65,536-byte input record ceiling
is a test protocol bound, not a package input restriction.
