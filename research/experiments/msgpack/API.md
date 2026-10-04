# MessagePack package

`lib/msgpack/modules.wfg` owns three modules: `types` (copyable events and
errors), `decode` (allocation-free pull decoding) and `encode` (append-only
writing). There is no Halo or Lua dependency. These modules separate the wire
reader and writer so consumers can select either independently; the common
representation records decoded meaning and retains the original tag.

Import the graph as a named package, for example
`package msgpack = "../../../lib/msgpack";`, then list the chosen modules as
dependencies. This document and experiment remain while this package is
maintained here; move them with its tests when it becomes a separate project.
The library files serve general binary serialization and move with the package.
The test runner serves reproducible compatibility checks and is retired only
when equivalent checks have a maintained replacement.

## Pull decoder

`decode::next(source: &[u8], offset: u64)` returns
`Result<Option<types::Token>, types::Error>`. `None` means exact EOF. A token
contains `tag`, `start`, `next`, and `event`. Pass `next` as the next offset.
An offset beyond EOF returns `Offset`; reserved byte 0xc1 returns `Reserved`.

Events cover nil, separate false and true, unsigned u64, signed i64, f32,
f64, string and binary payload ranges, array and map counts (u32), and an
extension's signed i8 type and payload range. Ranges are half-open absolute
input offsets. Retaining `tag` distinguishes all encoding widths. Strings are
raw bytes; the decoder does not validate UTF-8. Extensions are uninterpreted,
including the timestamp type. Floats retain their wire width and bit pattern.

A header yields its declared element or pair count without consuming children.
The caller tracks nesting and validates container completeness. Exact EOF
inside a container is therefore `None`, not a container-level error. There is
no recursion, depth limit, implicit object model or end-container event.

`Truncated { start, offset, needed, available }` identifies the token start,
the start of the incomplete length/value/type/payload segment, its required
byte count, and remaining input bytes there. A failed call changes no state.
A longer input can be retried at the same token offset.

## Writer

The writer appends to a caller-owned `Box<Slots<u8>>`, growing its capacity.
Typed integers preserve all 64 bits; nonnegative signed values use unsigned
encodings. Strings use fixstr/str8/str16/str32 at the same thresholds as Redis
7.0.15's bundled `lua_cmsgpack.c`. Number writing takes f64: an exactly
representable signed i64 is encoded as an integer; other numbers use float32
if converting to f32 and back equals the original, otherwise float64. Thus
negative zero becomes integer zero, infinities use float32, and NaNs use
float64. Explicit float functions preserve width instead of applying this
number policy. C's conversion of an out-of-range float to int64 is undefined;
this library uses a checked domain and never relies on that behavior.

Binary, extension, array and map writers choose their shortest header. Counts
are u32, the wire limit. Payloads exceeding u32 are refused rather than
truncated. Header-only array and map calls leave child ordering to the caller;
there is no Lua table classification, nesting policy or map key policy.
