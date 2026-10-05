# Halo embedding comparison

Compiler builds and checks have a persistent cache at
`${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/halo-e2e`.
`--cache DIR` overrides it; `--no-cache` disables caching. Cache paths must be
outside the repository and survive scratch-executable cleanup.
This runner reports program execution times, so native builds default to
`--full-lto`. The compiler rejects combining `--full-lto` with `--cache`;
`--incremental` selects the persistent cache instead. Cached runtime timings
have unvalidated differences from full LTO and are only sizing observations.
Use the default or explicit `--full-lto` for runtime performance measurements.
Reused binaries must have been built with the corresponding mode.

This explicitly invoked experiment runs the unchanged Halo oracle scripts through
`pkg::embed` and an in-memory Whitefoot Redis test host. It compares typed RESP2
JSON with the existing Redis 7.0.15 observations; it never regenerates them.
The runner owns fixture transport, JSON formatting and comparison, not Lua
execution. The Whitefoot host owns commands and both reply conversions.

The embedding module, test program, runner and result record serve VM.md section
6 and its first end-to-end comparison. They live in the existing Halo library
and experiment homes, and are removed or superseded when the production firn
binding replaces this test host or Halo is retired.

The runner never changes compiler, VM, heap or oracle files. Unsupported
commands and library functions are reported as failures rather than silently
removed from the corpus. See RESULTS.md for the measured coverage and gaps.

Run from the repository root, using an existing compiler (no Cargo):

```sh
python3 -B research/experiments/halo-e2e/run.py --compiler /path/to/whitefootc --incremental --filter lua-core/assert --budgets 1,7,1000 --report /private/tmp/halo-e2e-sample.md
python3 -B research/experiments/halo-e2e/run.py --compiler /path/to/whitefootc --full-lto --budgets 1,7,1000 --report /private/tmp/halo-e2e-results.md --actual /private/tmp/halo-e2e-replies
```

On 2026-10-05, macOS 26.6.2 arm64, the supplied gate compiler (SHA-256
`8d391bd75e31dbd2068f30e586c22cea59f10ef16b21b3f587d4364fec2beeb2`) built this worktree over parent
`4fcb0b289e8b5c76fcbb2f44031e29b6bf0f8acf` with `--incremental --budgets 1,7,1000`:
**480.935 s cold, 0.124 s unchanged, 233.842 s after a one-line body edit**.
Every run passed **240/240** comparisons. The default `halo-e2e` cache was
initially absent and retained between runs. The edit swapped `used + 1_u64`
for `1_u64 + used` in `lib/halo/embed/sha1.wf` and was reverted afterward.
These are compiler-invocation times, excluding oracle execution; cached runtime
speed relative to full LTO was not measured.

The module-fragment trial required an unchanged-build improvement before
adoption: ordinary cached linking took 0.133 s;
`--fragments module` took 9.573 s to populate and
0.221 s warm, and its binary passed 240/240. These single
trials did not show an improvement, so runners retain ordinary cached linking.
A separate relative-graph-path trial missed the absolute-path entry cache and
repeated front-end work; keep graph spelling consistent across measurements.

The runner builds the Whitefoot test executable once and runs every script with a
fresh engine/store. `--filter GROUP/NAME` selects a small sample. `--binary PATH`
reuses an executable; the caller must ensure it was built from the identified
source bytes. Both sides are parsed as typed reply objects, validated, and
rendered to the oracle's exact indented ASCII JSON before byte comparison; no
payload, integer, order, error location or nil kind is normalized. The runner
exits one for a mismatch and two for a build failure. Expected files remain
read-only. Python is used for binary transport and JSON comparison independently
of the Whitefoot implementation, not as a compiler or Lua interpreter.

The preparation chunk installs KEYS/ARGV from the JSON header and executes setup
commands through the same test host. A NUL separates it from the unchanged script
bytes on stdin. Global protection is applied afterward: raw host assignment
bypasses readonly, while script writes raise and absent reads call a rejecting
`__index`. The VM installs its slice-1 library; this host adds Redis members.

The `smoke` entry and `test` executable with three arguments run the embedding
probe (stdin is unused): cache, flush, reset, budget resumption, host outcomes,
and forced collection of pins and cached constants. A nonzero exit is the
probe's numbered failed observation. [GAPS.md](GAPS.md) names limits and concrete
reopening conditions. None of these research commands is a compiler gate.

The Redis error/SHA-1 comparison uses Redis 7.0.15's local `script_lua.c`
and `eval.c` as the formatting reference: command errors are tables, Lua
errors are strings, and the EVAL wrapper adds source/line and the SHA-1 of
the unchanged script body. SHA-1 is checked independently against Python's
`hashlib` on binary inputs, including block and padding boundaries. These
checks belong to this explicitly invoked experiment and leave oracle files
unchanged.

Add `--verify-sha1` to check three fixed and 1,000 seeded random binary
inputs against `hashlib` before the selected corpus. Add `--verify-errors`
to check command/global error locations, SHA-1 arity errors and protected
error values against Redis-source-grounded expectations. It exercises the same
Whitefoot `redis.sha1hex` used by scripts. `lib/halo/embed/sha1.wf` and
`redis-error.wf` serve digest generation and the EVAL reply formatter in
this embedding; they are superseded with the embedding if Halo is retired
or its production binding replaces these responsibilities.

Add `--gc-stress` to enable the engine's collector stress switch. Budget
arguments still use the existing positional protocol; `--gc-stress` is an
explicit flag received by the test executable and excluded from that count.
Completed collections during the tested script (excluding preparation) are
reported on stderr as JSON and included in each comparison row. The reply on
stdout keeps the original typed RESP2 schema. The switch defaults off.
See [the F4 experiment](../halo-gc/README.md#vm-stress-experiment-f4) for
missing-root mutation and memory-limit evidence.

`--cases PATH` runs paired local `scripts/GROUP/*.lua` and
`expected/GROUP/*.txt` files with the same comparator. `--verify-memory`
adds F4's unbounded allocator and then a recovery script in the same engine
and store; a second NUL separates that following script. `--collect-suspended`
adds one synthetic collector back-edge while a callback stack is parked,
before resume. `--isolate-frames` clears dead function-slot aliases at budget
checkpoints, leaving frame records to root executing closures. These last two
are explicit root-isolation probes; both default off. Their use and limits
are recorded in the F4 experiment.
