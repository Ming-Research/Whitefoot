# Halo / PUC Lua 5.1 lexer comparison

This experiment checks `lib/halo/lex` against Redis 7.0.15's bundled PUC
Lua 5.1.5 `llex.c`. It serves the Halo parser's token and diagnostic boundary;
it is explicitly invoked research, not a dependency of the compiler gate.

`run.py` builds the C oracle and the Whitefoot `dump` entry in a temporary
scratch directory. The oracle uses a copy of the reference C sources, with
one metadata-only insertion at the start of `llex`'s trivia loop to record
its current source offset and line. Token selection, decoding, numeral
validation and errors remain PUC's implementation. The compiler is supplied
externally; this experiment never builds it with Cargo.

The comparator checks token kind, half-open source byte range, starting line,
name/number source bytes, decoded string bytes and full lexical error text.
Payloads and errors use lowercase hex, so NUL and non-UTF-8 bytes survive.
A token record is `T<TAB>kind<TAB>start<TAB>end<TAB>line<TAB>payload`; a terminal
error is `E<TAB>message`. `-` means an empty or absent payload. EOF is included
in token counts. Whitefoot reads a batch from stdin: each source has a
four-byte big-endian initial line number (normally 1), a
four-byte big-endian chunk-name length, the chunk-name bytes, a four-byte
big-endian source length, and the source bytes. Each independent source ends
with an EOF token or an error record.

The corpus is every `*.lua` under `../halo-oracle/scripts`. Tricky inputs are
constructed in scratch by `run.py`: all keywords and symbols, numeral
boundaries and malformed numerals, every short-string escape including
unknown escapes and decimal boundaries, long delimiters and comments,
unterminated forms, mixed newline pairs, binary strings, each individual byte,
chunk abbreviations, seeded line-counter limit witnesses, buffer growth and fixed-seed token mixtures. Every
mismatch is printed with its file label and complete dump diff. Nonzero host
exit, timeout, an incomplete batch or any mismatch fails the command. Six
mutation controls check that changes to kind, range, line, payload, token
presence and error text are distinguished by the equality comparator.

## Run

From the worktree root, first size a one-file sample, then run the full set:

```sh
perl .github/run-check.pl halo-lex-sample python3 research/experiments/halo-lex/run.py --sample 1
perl .github/run-check.pl halo-lex-compare python3 research/experiments/halo-lex/run.py --output /private/tmp/halo-lex-results.json
```

Defaults match the supplied task environment. Override `--compiler` and
`--lua-source` to name the already-built Whitefoot compiler and the pinned
Redis Lua source directory. `--halo-dump` reuses a previously built comparison
entry. The scratch JSON is optional; the report is always printed. The C
compiler, Python standard library and Whitefoot host runtime are the only
other prerequisites. No network or Redis server is used.

## Lexer interface

`State` holds a byte offset, a one-based line counter and the previous token
kind (for PUC's line-limit diagnostic); construct it with
`new`, and pass the same source slice to each `next` call. All kinds use PUC's
numeric token codes; the public `tk_*` constants cover its reserved words and
multi-byte tokens. Every other source byte is its own single-character kind,
and 287 is EOF. `Token.line` is the token's starting line, even for multiline
strings; `Error.line` is the line on which PUC detects the failure.

The caller owns a reusable `Box<Slots<u8>>` passed as `decoded`. Only string
contents and lexical-error context use it; names and numbers refer directly
to their source range. Successful strings contain decoded bytes without
delimiters. The next call invalidates these bytes, so a parser copies/interns
a string before advancing. Geometric growth retains capacity between calls.
This avoids allocating a string owner for each token while keeping ownership
explicit under Whitefoot's prohibition on stored references. Cursor state,
scanning/decoding, token-table helpers and diagnostic formatting are split
into `lex.wf`, `scan.wf`, `support.wf` and `error.wf` under the module's existing
home; the parser consumes only `module.wfm`.

A returned `Error` owns its message bytes and line and describes the near-token
context. `Error.start` and `Error.end` cover the failed scan; `near_kind`
labels the diagnostic context, which can use the previous token at the line
limit. Errors are terminal; preserve the buffer and call `format_error`
with Lua's original chunk name (`@filename`, `=literal` or source text) and a
separate reusable output buffer. The formatter implements the 80-byte
`luaO_chunkid` abbreviation and diagnostic C-string truncation at NUL.
Successful decoded strings retain NUL.

Numerals are scanned exactly as `read_numeral`, including its trailing
alphanumeric/underscore consumption and C-string `check_next` NUL behavior.
The reference enables `LUA_COMPAT_LSTR=1`: nested level-zero `[[` openers
raise "nesting of [[...]] is deprecated" in strings and comments. Its line
limit is `MAX_INT = INT_MAX - 2`. Both behaviors are included in the checks.
The lexer checks the C-locale spelling reachable by that scan, including the
reference host's hexadecimal exponent forms, and returns no numeric value.
The parser owns conversion; `pkg::number` functions are not used. Character
classes match PUC in the initial C locale. Changing the embedding process's
C locale is outside this comparison.

The C oracle, graph, dump entry and Python comparator are one maintained
experiment. Remove them together when this independent comparison has a
replacement consumer; retain dated results as evidence. The module files
remain until Halo's lexer interface or implementation is superseded.
