# Redis Lua library compatibility results

Local revision: `2b6cc81c6e7f39051f3a269448facf4a30180bba`. Host: `macOS-26.6.2-arm64-arm-64bit-Mach-O`.
Reference: Redis 7.0.15 bundled sources, all four libraries explicitly registered; `2.1.0	lua-cmsgpack 0.4.0`.
Reference build 3.203s. Halo executable reused; no build performed during comparison. Budget 7.
Compiler SHA-256: `58b92b43013a5e6da17cb92fd6bf5124a0b80d7475d3b5dac1683ad8dc8e9a11`.
Executable SHA-256: `673901ad99e028dc31d41fa4c9e0c0c55455440cb3b8f6be3b4f567ff3da3f62`.

| Library | Snippets | Matches | Mismatches |
| --- | ---: | ---: | ---: |
| bit | 225 | 225 | 0 |
| cjson | 297 | 285 | 12 |
| cmsgpack | 157 | 156 | 1 |
| struct | 251 | 237 | 14 |

Total: 930 snippets; 903 matches; 27 mismatches.

The comparator checks typed replies, binary bytes, and exact error text. Its fault sensitivity controls come from the existing end-to-end runner. The first return value is converted as Redis RESP2; snippets wrap multiple results where needed. No oracle fixtures were changed. Local libc is not Linux glibc: glibc-dependent behavior remains unqualified by this run.

## Interpretation and remaining work

Byte-exact acceptance remains open. Seven mismatches are demonstrated missing
Lua debug names: cjson/221..227 cover calls through local aliases and the
field name in userdata operations. `Script` retains source lines but not
local-name ranges; completing those errors requires changes outside the
requested VM/library files. This is recorded in `docs/todo.md`.

The other twenty comparisons require the H4 Linux reference: two show
Darwin's `userdata: 0x0` versus Halo's intended glibc `userdata: (nil)`;
one packs 2^63 through a native out-of-range integer conversion; three use
an infinite numeric table key; fourteen pack values at or above 2^64 or
positive infinity as struct integers. The conversion differences are
suspected macOS/ARM versus x86-64 effects, not qualified Linux successes.
No installed offline Linux container/VM runner was available. The current
corpus retains every discrepant expectation unchanged.

CJSON instance settings, extracted methods, independent precision, collection
pressure and keep-buffer switching are tested, but unreachable instance
configuration slots and buffers are retained until the VM is destroyed.
That lifetime defect is recorded in `docs/todo.md`.
Neither `lib/json` nor `lib/msgpack` was changed. The general JSON decoder's
strict grammar/Unicode policy cannot directly supply Redis's permissive
forms and arbitrary bytes; the Halo normalization adapter bridges them.
The existing MessagePack token decoder and writer supply the required codec
operations beneath the Redis-specific tag and numeric policies.

## Stored oracle and independent codec observations

The required `halo-e2e/run.py --budgets 7 --filter libs/` run passed 7 of 10
cases; see [ORACLE.md](ORACLE.md). The three failures occur at line 7 in
`cmsgpack-binary`, `struct-integers` and `struct-strings-floats`, where the
unchanged fixtures call the absent `string.gsub`. Pattern functions are the
separate parallel task. The corpus additionally runs each fixture with its
hex-rendering operation expressed as a byte loop (cmsgpack/155, struct/249
and struct/250). All three match the standalone reference; their replies
were also compared to the original stored typed replies and matched. These
observations exercise the codecs; they do not change the original oracle's
failures or fixtures.

## Validation and observations

The supplied compiler built the existing `halo-e2e/modules.wfg` entry `test`
with `--fragments function` and a scratch persistent cache, exiting 0 on
`2b6cc81c6e7f39051f3a269448facf4a30180bba`. Both comparison commands then
used that executable and exited 1 because of the discrepancies reported
here and in ORACLE.md. No Cargo, network, push or pull request was used.
The supplied bundled Lua probe had no globals for any of the four libraries;
the runner therefore built the separately registered scratch reference.

Runs were sized first: a reference build took about three seconds; the first
eight native scalar cases took 3.2–4.8 ms each. The full corpus then ran at
budget 7. The shared comparator's sensitivity checks reject altered bytes,
integer values, reply kinds, array order/length and invalid reply schemas.
Fixes have observations that failed beforehand: missing error punctuation,
argument validation order, overflow precedence, protected-call names,
MessagePack raw key errors, 256-plus results and Lua C-stack boundaries.
For example, pack with 4,000 arguments returned length 4,000 on both engines;
4,001 returned length 4,001 before the fix and Redis's exact argument error
with the final executable.

The review repairs have 31 additional passing CJSON observations
(cjson/266..296), including actual vertical-tab/form-feed and embedded-NUL
bytes, library metadata, extracted-method collection pressure and independent
instance settings. Before repair, `cjson.decode("1\\011")` returned 1 instead
of Redis's invalid-token error, and the `on` option followed by NUL and `x`
was rejected instead of accepted. Both now match the reference.

`make static` and `git diff --check` passed on
`2b6cc81c6e7f39051f3a269448facf4a30180bba`. Earlier static attempts respected
another worktree's live check lock; the final run acquired it normally.
The canonical `make check` was not run under the no-Cargo constraint.
Linux/glibc execution, debug-name compatibility and instance reclamation
remain unverified or incomplete as described above.

## Independent review and design status

A separate read-only GPT-6 reviewer examined the complete change from
`d3fc122458f0028ef9d848ed152210539a502e5c`, checking A, D, C, M, V and
applicable R checklist items. Its source inspection, 22 differential probes,
digest verification, diff check and design lint found trailing VT/FF numeric
consumption, NUL option-prefix errors and a missing closure-representation
assessment. All three were addressed. A scoped follow-up inspected the
repairs, their consumers, regressions and draft decision and found no further
defects; the reviewer confirmed these bytes equal
`2b6cc81c6e7f39051f3a269448facf4a30180bba`. The fresh executable comparison
above supplies the runtime evidence that was pending during that review.

Q1 remains open: the provisional configured-method representation is recorded
in [the closure decision](../../../design/halo/closures.md), with
alternatives and reopening conditions in the
[closure binding assessment](README.md#closure-binding-assessment).
No owner approval or approval-log entry is inferred. There are no other tree
edits. The specification and conformance evidence are unchanged, so there
are no specification rules with before/after behavior to report.

## Mismatches

### cjson/211
```lua
local j=cjson.new(); j.encode_number_precision(2); return {j.encode(1.2345),cjson.encode(1.2345),type(j.null),tostring(j.null)}
```
Expected: `{"type": "array", "items": [{"type": "bulk", "bytes": "1.2"}, {"type": "bulk", "bytes": "1.2345"}, {"type": "bulk", "bytes": "userdata"}, {"type": "bulk", "bytes": "userdata: 0x0"}]}`
Actual: `{"type": "array", "items": [{"type": "bulk", "bytes": "1.2"}, {"type": "bulk", "bytes": "1.2345"}, {"type": "bulk", "bytes": "userdata"}, {"type": "bulk", "bytes": "userdata: (nil)"}]}`

### cjson/218
```lua
return {type(cjson.null),tostring(cjson.null),cjson.decode("null")==cjson.null,cjson._NAME,cjson._VERSION}
```
Expected: `{"type": "array", "items": [{"type": "bulk", "bytes": "userdata"}, {"type": "bulk", "bytes": "userdata: 0x0"}, {"type": "integer", "value": 1}, {"type": "bulk", "bytes": "cjson"}, {"type": "bulk", "bytes": "2.1.0"}]}`
Actual: `{"type": "array", "items": [{"type": "bulk", "bytes": "userdata"}, {"type": "bulk", "bytes": "userdata: (nil)"}, {"type": "integer", "value": 1}, {"type": "bulk", "bytes": "cjson"}, {"type": "bulk", "bytes": "2.1.0"}]}`

### cmsgpack/042
```lua
return cmsgpack.pack(9223372036854775808)
```
Expected: `{"type": "bulk", "bytes": "\u00cf\u007f\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u00ca_\u0000\u0000\u0000"}`

### cjson/221
```lua
local f=cjson.decode; return f(false)
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: bad argument #1 to 'f' (string expected, got boolean)"}`
Actual: `{"type": "error", "bytes": "ERR user_script:1: bad argument #1 to 'decode' (string expected, got boolean)"}`

### cjson/222
```lua
local f=bit.tobit; return f(false)
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: bad argument #1 to 'f' (number expected, got boolean)"}`
Actual: `{"type": "error", "bytes": "ERR user_script:1: bad argument #1 to 'tobit' (number expected, got boolean)"}`

### cjson/223
```lua
return cjson.null()
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: attempt to call field 'null' (a userdata value)"}`
Actual: `{"type": "error", "bytes": "ERR user_script:1: attempt to call a userdata value"}`

### cjson/224
```lua
return cjson.null+1
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: attempt to perform arithmetic on field 'null' (a userdata value)"}`
Actual: `{"type": "error", "bytes": "ERR user_script:1: attempt to perform arithmetic on a userdata value"}`

### cjson/225
```lua
return cjson.null[1]
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: attempt to index field 'null' (a userdata value)"}`
Actual: `{"type": "error", "bytes": "ERR user_script:1: attempt to index a userdata value"}`

### cjson/226
```lua
return cjson.null.."x"
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: attempt to concatenate field 'null' (a userdata value)"}`
Actual: `{"type": "error", "bytes": "ERR user_script:1: attempt to concatenate a userdata value"}`

### cjson/227
```lua
return #cjson.null
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: attempt to get length of field 'null' (a userdata value)"}`
Actual: `{"type": "error", "bytes": "ERR user_script:1: attempt to get length of a userdata value"}`

### cjson/228
```lua
return cjson.encode({[math.huge]=1})
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: Cannot serialise table: excessively sparse array"}`
Actual: `{"type": "error", "bytes": "ERR user_script:1: Cannot serialise number: must not be NaN or Inf"}`

### cjson/229
```lua
cjson.encode_invalid_numbers("null"); return cjson.encode({[math.huge]=1})
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: Cannot serialise table: excessively sparse array"}`
Actual: `{"type": "bulk", "bytes": "{\"null\":1}"}`

### cjson/230
```lua
cjson.encode_invalid_numbers(true); return cjson.encode({[math.huge]=1})
```
Expected: `{"type": "error", "bytes": "ERR user_script:1: Cannot serialise table: excessively sparse array"}`
Actual: `{"type": "bulk", "bytes": "{\"inf\":1}"}`

### struct/165
```lua
return struct.pack("b",18446744073709551616)
```
Expected: `{"type": "bulk", "bytes": "\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000"}`

### struct/166
```lua
return struct.pack("b",math.huge)
```
Expected: `{"type": "bulk", "bytes": "\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000"}`

### struct/175
```lua
return struct.pack("B",18446744073709551616)
```
Expected: `{"type": "bulk", "bytes": "\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000"}`

### struct/176
```lua
return struct.pack("B",math.huge)
```
Expected: `{"type": "bulk", "bytes": "\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000"}`

### struct/185
```lua
return struct.pack(">i3",18446744073709551616)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000"}`

### struct/186
```lua
return struct.pack(">i3",math.huge)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000"}`

### struct/195
```lua
return struct.pack("<i8",18446744073709551616)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`

### struct/196
```lua
return struct.pack("<i8",math.huge)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`

### struct/205
```lua
return struct.pack(">I8",18446744073709551616)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`

### struct/206
```lua
return struct.pack(">I8",math.huge)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`

### struct/215
```lua
return struct.pack("i16",18446744073709551616)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`

### struct/216
```lua
return struct.pack("i16",math.huge)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`

### struct/225
```lua
return struct.pack("I32",18446744073709551616)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`

### struct/226
```lua
return struct.pack("I32",math.huge)
```
Expected: `{"type": "bulk", "bytes": "\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u00ff\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`
Actual: `{"type": "bulk", "bytes": "\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000\u0000"}`
