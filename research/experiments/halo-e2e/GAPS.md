# Embedding boundaries exposed by the first comparison

These are integration gaps in the current Halo implementation, not Lua source
rejections or proposals to change the language. Reopen them before the firn
binding treats this experiment as full Redis compatibility. This record lives
with its reproducing runner because this task permits no changes to the VM or
project TODO.

## Collector roots

The VM collector's `mark_roots` scans the active constant pool, but has no
embedding root-list argument. Before each engine start/resume, the embedding
module rebuilds the unused tail of that pool from every cached script's original
constants and every host pin. The compiled constant ranges and indexes stay
unchanged. This makes host pins and inactive cached constants real roots without
adding globals that a Lua program could enumerate or changing VM files.

Pins are acquired/released while the engine is idle or suspended. A Host callback
receives the VM and stack, not the engine; it must keep values needed across
safepoints in the stack, globals or an already pinned container. Extending that
contract to pin/unpin *during* a running callback needs a VM-visible registry/root
list. VM.md section 2 specifies a registry, but the current VM exposes none.
Unpin removes the root at the next start/resume mirror refresh, not by collecting
immediately. Pin tokens are never reused; released slots remain nil until engine
destruction. Root mirroring consumes memory proportional to the cached constants
and pin token slots; this experiment does not measure that cost.

The probe forces collection to distinguish retention of a pin and an inactive
script's string from accidental survival, then forces another collection after
unpin to observe reclamation. Its result is reported with the experiment.

## Script identity and retained closures

ScriptIds pair a slot and a flush generation. Reset preserves the cache; flush
closes active upvalues, removes scripts and invalidates their IDs. Generation
exhaustion refuses subsequent compilation rather than reusing an identity.

VM `start` replaces its prototype and line metadata from one script. A closure
retained by the host or in a global table from one cached script cannot safely be
called by another script: prototype/code identities are script-relative. The
embedding currently follows that VM interface; it does not rebase/merge scripts.
A flush likewise cannot preserve callable closures of flushed scripts. Reopen
with a two-script witness retaining a closure across start, and assign script
ownership to closures or use one engine-wide code/prototype identity space before
supporting that use. Corpus cases run independent engines and do not verify it.

## Error locations and Stop

`format_error(engine, line)` formats the requested `@user_script:LINE: msg` and
preserves a location already present in a string error. `error_value` keeps the
original object. The caller supplies a known source line; zero means unknown.
The dispatch loop does not retain the failing PC for all error paths. After
unwinding, neither frames nor a prior budget checkpoint necessarily identifies
the failure. Exact Redis EVAL errors additionally carry the script SHA1 and the
`on @user_script:LINE` suffix. The test host reports real error messages, leaving
these location/wrapper differences visible in comparison rather than deriving a
location from the expected file or normalizing it away. Reopen when the VM
exports the failing PC/source information, including protected and callback
errors. Command errors caught by pcall retain their plain command message.

Ordinary host Stop returns the untouched call stack. VM `resume` uses a saved
budget checkpoint; the ordinary `HostOutcome::Stop` path in `calls.wf` does not
itself save a new checkpoint. Engine resume therefore refuses a HostStopped outcome instead of dispatching from
an unrelated checkpoint. The probe checks that refusal makes no further host call,
then resets the engine.
Reopen with a stop/resume witness before promising that continuation. The slice
protocol's stop-before-write/reset/restart is the currently useful protocol.

## Test host and conversion scope

The host uses separate tables for values, key types and expiry durations. It
supports GET, SET, INCR, DEL, EXISTS, EXPIRE, PEXPIRE, TTL, HSET, HGET, HGETALL,
LPUSH, RPUSH, LPOP, LRANGE, ZADD, ZCARD, ZSCORE, ZREMRANGEBYSCORE and PING. The
implemented forms serve the corpus, not the full Redis command option grammar.
Expiry uses a fixed clock at zero: positive durations remain live; nonpositive
ones delete immediately. Reopen with clock-valued expiry fixtures before using
this host to test countdowns. ZSCORE uses Halo's Lua number formatting; general
Redis double formatting and malformed score/expiry inputs are not verified.

RESP2 conversion truncates representable numbers toward zero, maps true to one and false/nil
to nil bulk, recognizes err before ok fields, and traverses tables from index
one until the first actual nil. False is an array element that converts to nil;
it does not end traversal. Numeric replies outside the signed 64-bit domain,
including nonfinite values, currently use INT64_MIN; Redis's target-specific C
conversion for those inputs has not been established by this corpus run.
Status/error payloads replace CR/LF with spaces and
use Redis's C-string boundary. A defensive reply nesting limit of 128 is a test
host limitation, not Lua acceptance. Cyclic/deep replies are not corpus cases.

The installed VM library is the eight current IDs in `vm/module.wfm`. Redis
call/pcall, status/error reply and log are supplied by this host. SHA1hex and the
additional Lua/codec libraries are absent and reported. Once the parallel
library's name/id table lands, feed its global and `library.member` rows to
`embed::install`; no VM or library files are modified here.
