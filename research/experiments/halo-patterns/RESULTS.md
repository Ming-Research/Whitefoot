# Halo Lua 5.1 patterns

Implementation and differential evidence for the four string pattern functions.
The test runner and cases in this directory consume the existing Halo end-to-end
host; they are retained while the pattern library is maintained and retired with
that capability.

## Implementation constraints

The reference is Redis 7.0.15's bundled Lua 5.1.5 `lstrlib.c`. Its matcher uses
32 capture slots, source offsets here replacing pointers, and a loop for its
`goto init` tail calls. Non-tail matcher recursion is bounded to 200 active
`match` calls, with `pattern too complex` at exhaustion, as explicitly requested.
The bundled source itself has no matcher-depth counter: `LUAI_MAXCCALLS=200`
is a VM C-call limit, not a pattern limit. Depth-bound tests therefore have a
separate contract expectation rather than claiming reference parity.

Matcher/class helpers are separate from allocation and VM calls. The iterator
uses an ordinary traced closure with three closed upvalues and a reserved
prototype sentinel, recognized only by VM call preparation. Substitution
callbacks use the existing nested activation and budget-continuation path;
substitution state and output remain in the VM while a callback is suspended.
These choices are within the requested VM/test file scope; no specification or
design-tree files are changed.

## Validation

Pending implementation and comparison. No network, Cargo, push or PR is used.
