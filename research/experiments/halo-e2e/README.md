# Halo embedding comparison

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
python3 -B research/experiments/halo-e2e/run.py --compiler /path/to/whitefootc --budgets 1,7,1000 --report /private/tmp/halo-e2e-results.md --actual /private/tmp/halo-e2e-replies
```

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
