# Postconditions over aggregate results

## Question

[FN-9] admits relation data from fragment-integer result ordinals, the
fragment-integer payload of a routed `Ok`, and measures reached from a result
place [CALL-4]; "Unit, float, aggregate, nested-payload, whole-Result, non-Ok,
and every other shape remains a legal ordinary result but cannot supply a
relation datum in this version." Separately, [MSR-3] gives exit-state meaning
in `ensures` only to measures of a written reference parameter, so a caller's
fact about an integer field of a struct does not survive a call whose row
writes that struct, even when the callee's postcondition restates it.

The Snowghost renderer met the limit three times in one day:

1. `atom_intern(table: &AtomTable, name: &[u8]) -> result: Result<Atom, AtomError>`
   (`struct Atom { public index: u32; }`) can only say in its doc string that
   "an Ok atom's index is below the length of table.spans"
   (Snowghost `design/vocabulary` at 2085324, `renderer/base/atom/module.wfm`).
   The document arena's `create_node` has the same shape with
   `NodeId { index: u32 }`, so every DOM operation reports a runtime
   `InvalidNode` that a proof could remove.
2. The PNG decoder's `parse_header` validates a `Header` struct and returns it
   in `Result<Header, PngError>`; `decode_png` checks width, height and bit
   depth again because no clause can reach a field of that payload
   (`trial/run-png` at 8ed3ae0, `renderer/image/png/chunks.wf:96`,
   `png.wf:17-61`).
3. The line breaker's `build_runs` returns six `Box<Slots<T>>` arrays grown in
   lockstep; `write_runs` and `push_run` keep guards whose false branch cannot
   happen (`trial/run-line-break` at 3b0af82, `renderer/text/line_break/runs.wf`,
   `line_break.wf:57-101`).

This investigation decides which extension, if any, removes those guards
without leaving the difference-bound fragment or adding a new kill rule. It
edits no specification, design tree or compiler.

## Method

Probes ran on the gate `whitefootc` built from Whitefoot `c84c4dd7a`, which
implements specification v0.76. This branch starts at `85e2c89bf` (v0.77); the
diff between the two changes FN-1, WAIT-1/2, CAP-1, HOST-1 and PAR-4 and none
of FN-8, FN-9, CALL-4, CALL-6, MSR-1..5, ENT-2..6 or INV-1. Every probe begins
with

```wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;
```

which the listings below omit. A probe given as an edit of another is that
file with exactly the stated lines replaced. `check` is `whitefootc --check`;
an accepted probe was also built and run.

## Probes and diagnostics

### Case 1: an identifier's validity

**p1a — unrouted struct result.**

```wf
struct Atom {
  index: u64;
}

struct Table {
  spans: Box<Slots<u32>>;
}

fn intern(table: &Table, key: u32) -> atom: Atom writes(table) contract {
  requires table^.spans.inner.len < table^.spans.inner.cap;
  ensures atom.index < table^.spans.inner.len;
} {
  let at = table^.spans.inner.len;
  place_back(window: &table^.spans.inner, value: key);
  return Atom(index: at);
}

fn main() -> status: ExitStatus pure {
  let spans = box_slots_new::<u32>(capacity: 4_u64);
  let table = Table(spans: move spans);
  let atom = intern(table: &table, key: 9_u32);
  let span = table.spans.inner[atom.index];
  let code = cvt.wrap::<u32, u8>(span);
  return exit_status(code: code);
}
```

```text
p1a_struct_result_field.wf:12:39: error[FN-9]: InvalidPostconditionSelector
  source: fn intern(table: &Table, key: u32) -> atom: Atom writes(table) contract {
  marker:                                       ^^^^^^^^^^
```

**p1b — routed `Ok` with a struct payload**, Snowghost's actual shape:

```wf
struct Atom {
  index: u64;
}

enum AtomError {
  Full();
}

struct Table {
  spans: Box<Slots<u32>>;
}

fn intern(table: &Table, key: u32) -> result: Result<Atom, AtomError> writes(table) contract {
  ensures when Ok(value: atom): atom.index < table^.spans.inner.len;
} {
  let at = table^.spans.inner.len;
  let room = at < table^.spans.inner.cap;
  if room {
    place_back(window: &table^.spans.inner, value: key);
    let atom = Atom(index: at);
    return Ok<Atom, AtomError>(value: atom);
  }
  let error = AtomError::Full();
  return Err<Atom, AtomError>(error: error);
}

fn main() -> status: ExitStatus pure {
  let spans = box_slots_new::<u32>(capacity: 4_u64);
  let table = Table(spans: move spans);
  let outcome = intern(table: &table, key: 9_u32);
  match outcome {
    Ok(value: atom) => {
      let span = table.spans.inner[atom.index];
      let code = cvt.wrap::<u32, u8>(span);
      return exit_status(code: code);
    }
    Err(error: problem) => {
      return exit_status(code: 1_u8);
    }
  }
}
```

```text
p1b_routed_ok_struct_payload.wf:17:16: error[FN-9]: InvalidPostconditionSelector
  source:   ensures when Ok(value: atom): atom.index < table^.spans.inner.len;
  marker:                ^^^^^^^^^^^^^^^
```

**p1c — the same relation over an integer result** is accepted and runs (exit
9): p1a with `-> atom: Atom` replaced by `-> index: u64`, the clause by
`ensures index < table^.spans.inner.len;`, `return Atom(index: at);` by
`return at;`, and the caller's `let atom = intern(...)` / `[atom.index]` by
`let index = intern(...)` / `[index]`.

**p1f — the fact survives a later write when the callee says how the table
grows.** p1c without `struct Atom`, with two further clauses

```wf
  ensures table^.spans.inner.len == entry(table)^.spans.inner.len + 1_u64;
  ensures table^.spans.inner.cap == entry(table)^.spans.inner.cap;
```

and a caller that interns twice and then indexes with both results
(`let first = intern(...); let second = intern(...); ... inner[first] ...
inner[second]`, summed with `+wrap`). Accepted, runs (exit 14). The first
index's bound crosses the second `writes(table)` call through the
`entry(table)` call datum [ENT-3.S13]: the integer-result form of case 1 is
complete in the current language.

**p1d, p1e — cross-width relations.** Snowghost's index is `u32` and the
measure is `u64`. p1a with `index: u32`, result `-> index: u32`, clause
`ensures cvt::<u32, u64>(index) < table^.spans.inner.len;`:

```text
p1d_cross_width_relation.wf:14:11: error[FN-9]: InvalidPostconditionRelation
  source:   ensures cvt::<u32, u64>(index) < table^.spans.inner.len;
```

and without the conversion (p1e), `ensures index < table^.spans.inner.len;`:

```text
p1e_cross_width_bare.wf:14:19: error[TYPE-5]: TypeMismatch
  expected: own u32
  found: own u64
```

**p1g — construction carries no field value, even into a `requires`.**

```wf
struct NodeId {
  index: u64;
}

struct Document {
  nodes: Box<Slots<u8>>;
}

fn kind_of(document: &Document, node: NodeId) -> kind: u8 reads(document) contract {
  requires node.index < document^.nodes.inner.len;
} {
  let at = node.index;
  let kind = document^.nodes.inner[at];
  return kind;
}

fn main() -> status: ExitStatus pure {
  let nodes = box_slots_new::<u8>(capacity: 4_u64);
  place_back(window: &nodes.inner, value: 3_u8);
  let document = Document(nodes: move nodes);
  let root = NodeId(index: 0_u64);
  let kind = kind_of(document: &document, node: root);
  return exit_status(code: kind);
}
```

The callee is accepted (a parameter field is already a requirement datum);
the caller is not, because `root.index` has no relation to `0_u64`:

```text
p1g_construct_field_to_requires.wf:25:14: error[FN-8]: UndischargedCallRequirement
  requires_clause: ... "requires node.index < document^.nodes.inner.len;"
  instantiated_goal: root.index < document.nodes.inner.len
  disposition: Unproved
```

**p1h** is p1g with `index: u32`, `requires cvt::<u32, u64>(node.index) <
document^.nodes.inner.len;`, `let at = cvt::<u32, u64>(node.index);` and
`NodeId(index: 0_u32)`. The requirement forms, but gives the body no usable
fact, because a conversion is no [ENT-2] term and the goal has no L0
projection:

```text
p1h_cross_width_requires.wf:16:35: error[OP-4]: UndischargedBoundsObligation
  residual: at < document^.nodes.inner.len
```

### Case 2: a validated header

**p2a — routed struct payload.**

```wf
struct Header {
  width: u32;
  height: u32;
}

enum PngError {
  Malformed();
}

fn parse_header(width: u32, height: u32) -> result: Result<Header, PngError> pure contract {
  ensures when Ok(value: header): header.width >= 1_u32;
  ensures when Ok(value: header): header.width <= 16384_u32;
  ensures when Ok(value: header): header.height >= 1_u32;
  ensures when Ok(value: header): header.height <= 16384_u32;
} {
  let width_ok = width >= 1_u32;
  let width_small = width <= 16384_u32;
  let height_ok = height >= 1_u32;
  let height_small = height <= 16384_u32;
  let sizes = band(width_ok, width_small);
  let heights = band(height_ok, height_small);
  let valid = band(sizes, heights);
  if valid {
    let header = Header(width: width, height: height);
    return Ok<Header, PngError>(value: header);
  }
  let error = PngError::Malformed();
  return Err<Header, PngError>(error: error);
}

fn main() -> status: ExitStatus pure {
  let parsed = parse_header(width: 3_u32, height: 2_u32);
  match parsed {
    Ok(value: header) => {
      let stride = header.width * 4_u32;
      let bytes = stride * header.height;
      let code = cvt.wrap::<u32, u8>(bytes);
      return exit_status(code: code);
    }
    Err(error: problem) => {
      return exit_status(code: 1_u8);
    }
  }
}
```

```text
p2a_header_ok_payload_fields.wf:14:16: error[FN-9]: InvalidPostconditionSelector
  source:   ensures when Ok(value: header): header.width >= 1_u32;
  marker:                ^^^^^^^^^^^^^^^^^
```

**p2c** is p2a with the four clauses and the `contract` block removed. The
caller then fails where Snowghost keeps its guard, and the repair names a
postcondition FN-9 cannot express:

```text
p2c_header_no_contract.wf:33:20: error[OP-2]: UndischargedIntegerDomainObligation
  source:       let stride = header.width * 4_u32;
  residual: header.width *defined 4_u32
  mechanical_fix: ... when the callee whose result it reads can prove the bound,
    state it in that callee's `ensures`; ...
```

**p2d — integer tuple workaround.** Accepted, runs (exit 24):

```wf
enum PngError {
  Malformed();
}

fn parse_header(width: u32, height: u32) -> (status: Result<unit, PngError>, checked_width: u32, checked_height: u32) pure contract {
  ensures checked_width >= 1_u32;
  ensures checked_width <= 16384_u32;
  ensures checked_height >= 1_u32;
  ensures checked_height <= 16384_u32;
} {
  let width_ok = width >= 1_u32;
  let width_small = width <= 16384_u32;
  let height_ok = height >= 1_u32;
  let height_small = height <= 16384_u32;
  let sizes = band(width_ok, width_small);
  let heights = band(height_ok, height_small);
  let valid = band(sizes, heights);
  if valid {
    return Ok<unit, PngError>(value: unit), width, height;
  }
  let error = PngError::Malformed();
  return Err<unit, PngError>(error: error), 1_u32, 1_u32;
}

fn main() -> status: ExitStatus pure {
  let (outcome, width, height) = parse_header(width: 3_u32, height: 2_u32);
  match outcome {
    Ok(value: done) => {
      let stride = width * 4_u32;
      let bytes = stride * height;
      let code = cvt.wrap::<u32, u8>(bytes);
      return exit_status(code: code);
    }
    Err(error: problem) => {
      return exit_status(code: 1_u8);
    }
  }
}
```

The published bounds suffice for both multiplications: once the fields are
relation data, the caller's `4 * width` needs no scaled postcondition. The
workaround costs a dummy `1, 1` on the error exit, because unrouted clauses
hold on every return.

**p2b — a scaled relation.**

```wf
fn row_bytes(width: u64) -> stride: u64 pure contract {
  requires width <= 16384_u64;
  ensures stride == 4_u64 * width;
} {
  let stride = width * 4_u64;
  return stride;
}
```

```text
p2b_scaled_relation.wf:6:11: error[FN-9]: InvalidPostconditionRelation
  source:   ensures stride == 4_u64 * width;
```

**p2f, p2g — no field value inside one body.**

```wf
struct Header {
  width: u32;
  height: u32;
}

fn main() -> status: ExitStatus pure {
  let w = 7_u32;
  let header = Header(width: w, height: 1_u32);
  let wide = header.width * 4_u32;
  let code = cvt.wrap::<u32, u8>(wide);
  return exit_status(code: code);
}
```

```text
p2f_construct_field_fact.wf:12:14: error[OP-2]: UndischargedIntegerDomainObligation
  residual: header.width *defined 4_u32
```

```wf
fn main() -> status: ExitStatus pure {
  let header = Header(width: 0_u32, height: 1_u32);
  set header.width = 7_u32;
  let direct = header.width * 4_u32;
  let second = header;
  let copied = second.width * 4_u32;
  let sum = direct +wrap copied;
  let code = cvt.wrap::<u32, u8>(sum);
  return exit_status(code: code);
}
```

```text
p2g_rebind_field_fact.wf:14:16: error[OP-2]: UndischargedIntegerDomainObligation
  source:   let copied = second.width * 4_u32;
```

`header.width` is already an [ENT-2] clause (a) term: the `set` gives it a
value and `direct` discharges. What is missing is transport: a construction
or a rebinding of the struct carries nothing to the destination's field.

**p2e — field atoms in invariants.**

```wf
fn main() -> status: ExitStatus pure {
  let header = Header(width: 1_u32, height: 1_u32);
  for (
    i in 0_u64..3_u64,
    invariant narrow: header.width <= 16384_u32
  ) {
    let fresh = Header(width: 2_u32, height: 2_u32);
    set header = fresh;
  }
  let code = cvt.wrap::<u32, u8>(header.width);
  return exit_status(code: code);
}
```

```text
p2e_field_atom_in_invariant.wf:13:23: error[INV-1]: InvalidInvariant
  reason: an affine factor selects a field or an element of a place
```

Snowghost's `parse_chunks` keeps its header in such a loop
(`set header = parsed_header;`, `chunks.wf:285-286`).

### Case 3: parallel arrays

**p3a — conditional lockstep growth.**

```wf
struct Runs {
  classes: Box<Slots<u8>>;
  starts: Box<Slots<u64>>;
}

fn push_run(runs: &Runs, class: u8, start: u64) -> result: unit writes(runs) contract {
  requires runs^.classes.inner.len < runs^.classes.inner.cap;
  requires runs^.starts.inner.len < runs^.starts.inner.cap;
  ensures runs^.classes.inner.len == entry(runs)^.classes.inner.len + 1_u64;
  ensures runs^.starts.inner.len == entry(runs)^.starts.inner.len + 1_u64;
  ensures runs^.classes.inner.cap == entry(runs)^.classes.inner.cap;
  ensures runs^.starts.inner.cap == entry(runs)^.starts.inner.cap;
} {
  place_back(window: &runs^.classes.inner, value: class);
  place_back(window: &runs^.starts.inner, value: start);
  return unit;
}

fn build_runs(text: &[u8]) -> result: Runs reads(text) contract {
  requires text^.len <= 1073741824_u64;
  ensures result.starts.inner.len == result.classes.inner.len;
} {
  let length = text^.len;
  let classes = box_slots_new::<u8>(capacity: length);
  let starts = box_slots_new::<u64>(capacity: length);
  let runs = Runs(classes: move classes, starts: move starts);
  for (
    i in 0_u64..length,
    invariant same: runs.starts.inner.len == runs.classes.inner.len,
    invariant bounded: runs.classes.inner.len <= i,
    invariant cap_c: runs.classes.inner.cap == length,
    invariant cap_s: runs.starts.inner.cap == length
  ) {
    let class = text^[i];
    let keep = class != 0_u8;
    invariant probe: runs.starts.inner.len <= i;
    if keep {
      push_run(runs: &runs, class: class, start: i);
    }
  }
  return move runs;
}

fn write_runs(runs: &Runs) -> total: u64 reads(runs) contract {
  requires runs^.starts.inner.len == runs^.classes.inner.len;
} {
  let count = runs^.classes.inner.len;
  let sum = 0_u64;
  for (r in 0_u64..count) {
    let start = runs^.starts.inner[r];
    set sum = start;
  }
  return sum;
}

fn main() -> status: ExitStatus pure {
  let backing = box_array_filled::<u8>(count: 3_u64, value: 7_u8);
  let text = &backing.inner[0_u64..3_u64];
  let runs = build_runs(text: text);
  let last = write_runs(runs: &runs);
  let code = cvt.wrap::<u64, u8>(last);
  return exit_status(code: code);
}
```

```text
p3a_conditional_lockstep_push.wf:32:5: error[INV-1]: UndischargedLoopInvariant
  source:     invariant same: runs.starts.inner.len == runs.classes.inner.len,
  name: same
  obligation: Backedge
  required_relation: runs.starts.inner.len <= runs.classes.inner.len
```

This is the writer's reported INV-1 backedge failure. (Without the body
`invariant probe` the call's second requirement is reported unproved first:
the automatic derivation did not chain the five header facts, and the written
invariant is accepted.)

**p3d** is p3a with `let keep = ...;` removed and the `if keep { ... }` replaced
by its unconditional `push_run(...)`. It is accepted and runs (exit 2),
**including `build_runs`'s `ensures result.starts.inner.len ==
result.classes.inner.len` and `write_runs` reading `starts.inner[r]` for every
`r < classes.inner.len` with no guard.** The measure-chain postcondition the
writer first tried is already admitted [CALL-4]; case 3's refusal is not
FN-9's.

**p3h** is p3a with the equality restated after the call and in an added
`else` arm (`invariant eq_after: ...; } else { invariant eq_else: ...; }`);
the same backedge failure remains.

**p3f — scalar reduction.**

```wf
fn main() -> status: ExitStatus pure {
  let a = 0_u64;
  let b = 0_u64;
  for (
    i in 0_u64..10_u64,
    invariant same: a == b,
    invariant bounded: a <= i
  ) {
    let keep = i != 3_u64;
    if keep {
      set a = a + 1_u64;
      set b = b + 1_u64;
    }
  }
  let code = cvt.wrap::<u64, u8>(a);
  return exit_status(code: code);
}
```

```text
p3f_scalar_conditional_lockstep.wf:9:5: error[INV-1]: UndischargedLoopInvariant
  name: same
  obligation: Backedge
  required_relation: a <= b
```

**p3g — the same join with an L0 source** is accepted and runs (exit 2):

```wf
fn step(a: u64, b: u64, keep: Bool) -> total: u64 pure contract {
  requires a == b;
  requires a <= 100_u64;
} {
  if keep {
    set a = a + 1_u64;
    set b = b + 1_u64;
  }
  invariant joined: a == b;
  return a;
}
```

**p3b — a count field of the result.**

```wf
struct Runs {
  count: u64;
  classes: Box<Slots<u8>>;
  starts: Box<Slots<u64>>;
}

fn empty_runs(capacity: u64) -> result: Runs pure contract {
  requires capacity <= 1073741824_u64;
  ensures result.count == result.classes.inner.len;
  ensures result.count == result.starts.inner.len;
} {
  let classes = box_slots_new::<u8>(capacity: capacity);
  let starts = box_slots_new::<u64>(capacity: capacity);
  let runs = Runs(count: 0_u64, classes: move classes, starts: move starts);
  return move runs;
}
```

```text
p3b_count_field_of_result.wf:12:11: error[FN-9]: InvalidPostconditionSelector
  source:   ensures result.count == result.classes.inner.len;
  marker:           ^^^^^^
```

**p3c — a count field of a written reference parameter**, the "related"
limit:

```wf
struct Runs {
  count: u64;
  classes: Box<Slots<u8>>;
}

fn push_run(runs: &Runs, class: u8) -> result: unit writes(runs) contract {
  requires runs^.count == runs^.classes.inner.len;
  requires runs^.classes.inner.len < runs^.classes.inner.cap;
  ensures runs^.count == runs^.classes.inner.len;
} {
  place_back(window: &runs^.classes.inner, value: class);
  set runs^.count = runs^.count + 1_u64;
  return unit;
}
```

```text
p3c_count_field_of_written_parameter.wf:16:3: error[FN-9]: UndischargedPostcondition
  postcondition: ... "ensures runs^.count == runs^.classes.inner.len;"
  relation: runs^.count = runs^.classes.inner.len
  disposition: Unproved
```

`runs^.classes.inner.len` denotes the exit state, but `runs^.count` denotes the
entry image [FN-9, MSR-3], whose stability the body's own `set` ends. The body
re-establishes the relation and cannot say so; the caller's fact over
`runs.count` dies at the call's projected write and nothing replaces it.

### Summary

| Probe | Shape | Result |
|---|---|---|
| p1a, p3b | integer field of a struct result | FN-9 InvalidPostconditionSelector |
| p1b, p2a | integer field of a routed `Ok` struct payload | FN-9 InvalidPostconditionSelector |
| p1c, p1f, p2d | same relations over integer results | accepted |
| p1d, p1e | `u32` datum against a `u64` measure | FN-9 InvalidPostconditionRelation / TYPE-5 |
| p1g, p2f, p2g | field value after construction or rebinding | no fact (FN-8, OP-2) |
| p1h | widening conversion in a `requires` | no usable fact (OP-4) |
| p2b | `stride == 4 * width` | FN-9 InvalidPostconditionRelation |
| p2e | `header.width` in a loop invariant | INV-1 InvalidInvariant |
| p3c | integer field of a written reference parameter | FN-9 UndischargedPostcondition |
| p3d | equal measures of a struct result | accepted |
| p3a, p3f, p3h | lockstep growth under a branch in a loop | INV-1 Backedge |
| p3g | same join, L0 source | accepted |

## Why the limit exists

**Not the fragment.** `result.width <= 16384` and `atom.index < table^.len`
are one datum and a constant per side: exactly the difference bound [ENT-4]
closes. The fragment is what refuses `4 * width` (coefficient four) and
`width * height <= 2^26` (two data, nonlinear), and the design tree's
contracts node already refuses widening a clause side to two data
(`design/language/checks-and-proofs/requires-entry-contract.md`, "One side of
a contract clause carries one datum").

**Transport.** FN-9 needs, at every selected return, a term for each named
result datum "read from its own ordinal's returned expression", and at the
caller a destination term to substitute. For a struct result both ends
exist: `h.width` over a tracked binder is an [ENT-2] clause (a) term (p2g's
`direct` shows it carries facts), and so is `result`'s returned place. What
does not exist is the path between them. [MSR-3]'s placement table carries a
value across `let`/`set` rebinding, construction, destructuring and payload
selection only as *measures* of the owned descendants; [ENT-3.S5] relates
only fragment-typed bindings. So a body that builds `Header(width: w, ...)`
and returns it knows nothing about the returned field (p2f), and a caller
that stores or rebinds the struct loses it (p2g). The Ok route has a second
gap: [ENT-5]'s conditional Result context carries one private *integer*
payload parameter (ENT-2 clause (i); `TermKind::ResultPayload(IntegerType)`
in `compiler/src/semantic/entailment/term.rs`), so a struct payload has no
term for a relation to name. When measures of result descendants were
admitted (log entry 2026-09-22, "Carry owned descendant measures"; tree
decision "An ensures clause may name a measure reached from a result
place..."), the stated ground was that "the transport already exists"; for
integer leaves it did not, and "in this version" in FN-9 marks a deferral,
not a refusal on soundness.

**Soundness of field images.** A field of a tracked place changes only by an
event that writes that field or a prefix of it — a `set`, `swap`, update,
consume, scope exit or projected callee write — and [ENT-5] kills every
fact supported by an overlapping place; a sibling write overlaps nothing
[MSR-2]. No new kill rule is needed. The real boundaries are the ones measures
already observe: a mutable field below a subscript is no term (ENT-2 clause
(b) admits only readonly fields there), an enum-payload step of an unrouted
result names no selected variant, and a result stored into aggregate or
indexed storage has no admitted destination [FN-9 `M(c,q)`]. An extension
that keeps those boundaries adds no unsound shape.

**Cost.** A measured place carries at most three measures; a struct can hold
any number of integer leaves. The compiler's measure placements mint one
datum set for every measured place under the operand
(`MeasureDatum` in `term.rs`). Carrying every integer leaf the same way would
multiply terms at every rebind of a wide struct, which is a plausible reason
the family stopped at measures. [MSR-3] already says the vocabulary formed at
the event suffices; applying that literally bounds the cost (below).

**Exit-state integer fields.** MSR-3 states "this former adds no scalar
snapshot family": non-measure parameter datums keep the entry-image
judgment. For a reference parameter the row writes, the entry image of an
integer field is unavailable after any overlapping body write and dies at
the caller's projected write, so today such a clause is either unprovable in
the body (p3c) or useless at the caller. Nothing in the specification relies
on that meaning.

## Proposal

One principle: **a fragment-integer field reached through an owned descendant
projection is treated wherever an [MSR-1] measure of an owned descendant
already is**, with the same destinations, placements, kills and denotations,
and no new relation form. Parts A–D are the aggregate extension; E and F are
independent and separately judged.

**A. Admission [FN-9, CALL-4].** A result ordinal of struct type supplies, as
relation data, each place reached from its binder by an owned descendant
projection of struct-field selections and `Box` `inner` steps ending at a
fragment-integer field (`result.width`, `made.header.width`,
`result.inner.count`) or, as today, at a measure member. The bare struct
binder is no datum. A routed `when Ok(value: r):` is admitted when the
payload type T is a fragment integer (today) or a type an unrouted ordinal of
type T would admit, and r supplies exactly the data such an ordinal would.
Enum-payload steps, subscripts, dereferences and type-parameter steps stay
refused, as for measures. Each operand remains one datum plus a constant.

**B. Selected-return images [FN-9].** At a selected return, `result.p.f`
evaluates to the returned expression projected by `p.f`: for a returned place
`q`, the tracked place `q.p.f`; for a returned construction, the field
operand's term or constant (a construction operand is an atom [GRAM-9]); for
`Ok(value: q)`, `q.p.f`. A forwarded Result uses the conditional context of D.
A projection whose endpoint is no [ENT-2] term leaves that clause unproved,
exactly as today.

**C. Placement transport [MSR-3].** The owned descendant projection may end at
a fragment-integer field as well as at the first measured type, and a
placement datum denotes "one measure of it, or its value", as a call datum
already does (`CallDatum.measure: Option<...>` in the compiler). The five
placements of the table then carry integer leaves: p2f's construction
establishes `header.width == w`, p2g's rebind `second.width == header.width`.
To bound cost, a placement mints a value datum only for a leaf whose source
term occurs in the pre-kill closed state; any other leaf has only its type
bounds, which hold at the destination without transport. This is the
sentence MSR-3 already states for measures, applied as a minting rule.

**D. Caller destinations and the Ok context [CALL-6, ENT-2, ENT-5].** The
destination list of [ENT-3.S12] is unchanged; `result.p.f` is substituted by
`d.p.f` at the let binder, `set` target place or destructuring binder d, which
is an [ENT-2] term exactly when d is a tracked place, so an aggregate-stored
or indexed destination stays unavailable. The conditional Result context's
private parameter becomes a private root typed by the Ok payload, whose
fragment-integer and measure projections are its terms; success selection
substitutes the receiving binder for that root, so `p.width` becomes
`header.width`. Transport, joins, kills and loop rules of the context are
unchanged; only its vocabulary widens.

**E. Widening conversions in relation terms [FN-9, FN-8, ENT-2].** Relations
are over mathematical values [ENT-2], yet p1d/p1e/p1h show no way to relate a
`u32` datum to a `u64` measure. Admit a bare `cvt::<S, D>(d)` of a datum d,
for a pair whose every S value is a D value, as d itself in a relation term
and in a requirement's L0 projection. Without E, case 1 must widen
`Atom.index` and `NodeId.index` to `u64`.

**F. Exit state of integer fields of written reference parameters [MSR-3,
FN-9].** Extend MSR-3's denotation table: a bare fragment-integer field of a
reference parameter whose row writes it denotes the selected return's exit
state, and `entry(parameter)^.p.f` its entry datum, exactly the two rows
measures have. p3c then proves, and its caller keeps `runs.count ==
runs.classes.inner.len` across the call. Every clause proved today stays
proved: its entry image was stable, so the field was not written and its exit
value is its entry value; its caller gains the relation instead of losing it
to the projected write.

### What fits the fragment and what does not

| Relation | Fits | Note |
|---|---|---|
| `atom.index < table^.spans.inner.len` | yes (A, B, D; E for `u32`) | the table's growth relation already exists (p1f) |
| `header.width <= 16384`, `>= 1` | yes (A–D) | removes the range and zero checks |
| `result.starts.inner.len == result.classes.inner.len` | yes, **today** | p3d; k arrays need k−1 clauses |
| `result.count == result.classes.inner.len` | yes (A–C) | two data, one per side |
| `runs^.count == runs^.classes.inner.len` over a written parameter | yes (F) | |
| `stride == 4 * width` | no | coefficient four; the caller recomputes `width * 4` and discharges its domain from the published bound (p2d) |
| `width * height <= 2^26` | no | nonlinear; decode_png's area check stays unless the header also returns its area and the consumer does not need the product identity |
| a `NodeId` read back from a node's `Links` | no | no quantified element facts (tree decision in `checks-and-proofs.md`); an identifier loaded from storage needs its own check |
| "if `header_seen` then `width >= 1`" | no | no implication facts; initialise the header to a valid default or carry scalars |

### What it must still refuse

A negative conformance case for each: an enum-payload step of an unrouted
struct result; a float or Bool field; a field of a result stored into an array
element and read back; a destination field written after the call (the
relation dies); a consumed destination; a sibling-field write (the relation
survives — a positive twin); a non-widening `cvt::<u64, u32>` operand; a
struct type-parameter step in a generic template; `4 * width` and a two-datum
side (unchanged); an Ok clause over a forwarded Result with no transported
context (unproved); and F's entry/exit distinction (`entry(runs)^.count`
after `set runs^.count = ...` equals the old value, not the new).

## Separate finding: lockstep growth under a branch

Case 3's guards do not depend on this proposal. p3d proves the equal-length
relation with today's rules; p3a, p3h and the scalar p3f fail because of a
join, and p3g shows the same join succeeding when the relation is an L0 fact.
The reading of the specification that explains all four:

- a loop's continuing kills replace each loop-carried binding by a fresh
  header image, and "proved header invariants are the only source-written
  relations reintroduced over those header images" [ENT-6]; an invariant's
  conclusion is "one published affine fact" over value images [INV-1], with
  no L0 projection;
- at a join, a binding whose images differ receives a common form plus a
  fresh delta atom, or a fresh atom [ENT-6], independently per binding, so
  `a` and `b` get unrelated deltas; an affine fact survives only if the
  canonically identical inequality reaches every input [INV-1], which it
  cannot once one arm changed both images;
- the L0 weakest-bound join [ENT-5] would keep `a - b <= 0`, but no L0 fact
  over the place terms exists at the loop head, so the updated arm never
  derives one either. A `requires` is an S4 source with an exact L0
  projection, which is why p3g passes.

So the refusal follows the specification as written rather than a compiler
defect. A candidate repair, not evaluated here: a proved invariant whose
normalized target is a unit-coefficient difference bound over live place or
measure terms also establishes that L0 relation over the current place terms,
as S4 does for a requirement, killed by ordinary [ENT-5] events. At the proof
point those terms denote exactly the images the invariant was proved over, so
the projection is true there. It needs its own soundness and cost review; this
evidence is recorded against the existing `docs/todo.md` item "Conditional
measure preservation needs a precise remaining diagnosis".

## Alternatives

- **Keep integer-only results and return tuples of integers (p2d).**
  Rejected. It works for one hop, but unrouted clauses must hold on the error
  exit too, so the callee invents dummy values; a `Result` routes only one
  integer; the caller must reassemble the nominal struct, and construction
  then drops the facts (p2f), so they never reach the `ParsedChunks` the
  decoder actually receives. It also gives up the nominal `Atom`/`NodeId`
  types that keep identifiers from mixing with other integers.
- **Type-level invariants on structs** (a declared relation every value of the
  type satisfies). Rejected: the tree already records that privacy "adds no
  implicit type invariant" and that naming a path establishes no fact
  (`checks-and-proofs.md`), every construction and field write would owe the
  invariant again, and the relations here are to *another* value (a table's
  length), which a type cannot state. The refined-integer idea in
  `docs/ideas.md` would give case 2's constant bound but not cases 1 or 3.
- **Admit `result.f` in FN-9 without C.** Rejected: it admits clauses that an
  ordinary body building its result by construction cannot prove (p2f), and
  facts that die at the caller's first rebinding (p2g).
- **Carry every integer leaf at every placement,** as measures are carried.
  Rejected for cost: the vocabulary rule in C carries the same provable facts
  and adds no terms to programs that name no field in a relation.
- **Scaled or multi-datum postconditions for `4 * width`.** Rejected: already
  refused by the contracts decision, and unnecessary once the bound itself is
  published (p2d).
- **Getters with postconditions.** Rejected: a getter's `ensures` constrains
  its call's result, not `h.width` at later reads, and calls stay excluded
  from contract expressions (`checks-and-proofs.md`).
- **Leave it.** Rejected: the three modules keep branches whose false edge is
  not intended behavior, which the tree's rule "an always-true relation is a
  verified contract or invariant" (`checks-and-proofs/obligation-discharge.md`)
  exists to exclude, and Snowghost's docs now explain those branches as
  checker limits.
- **Field atoms in INV-1 (p2e)** are not needed for the three cases: a loop
  can carry `width` and `height` as scalars and construct the header after
  it. Defer until a program needs a loop-carried struct field in an invariant.

## Cost estimate

**Specification** (one amendment, v0.78): FN-9 (admission sentence, routed
payload rule, the "unit, float, aggregate ..." list), CALL-4 (what a struct
ordinal supplies), MSR-3 (projection endpoint, placement-datum identity, the
minting rule, two denotation rows for F, and the "no scalar snapshot family"
sentence), ENT-2 clauses (h) and (i), ENT-5's conditional transport opening
sentence, and E's term rule. About 30–45 changed normative lines, with no new
rule ID. Conformance: `fn9-neg-aggregate-field-result-selector` changes
verdict with the amendment (its shape becomes admitted; a Bool-field negative
replaces it), and roughly 16 new cases: 8 positive (unrouted field, nested
field, `Box` content field, routed Ok payload with match and with propagate,
construction and rebind placements, destructuring destination, `set`
destination), 6 negative from the list above, and 2 each for E and F.

**Compiler** (reference sizes: admitting result-place measures, `3e7ce0e13`,
77 changed lines; `Box` content measure placements, `d41c1a014`, about 250
source and 320 test lines):

- `semantic/check/ensures.rs`: selector admission and routed payload types
  (~100–150 lines);
- `semantic/entailment/term.rs` and users: `MeasureDatum`/`EntryDatum` gain an
  optional measure like `CallDatum`; `ResultPayload(IntegerType)` becomes a
  projected payload root (31 uses in 11 files);
- `semantic/entailment/flow/sources.rs`, `results.rs`, `postconditions.rs`:
  integer-leaf placements with the vocabulary rule, selected-return
  projection, destination substitution;
- `events.rs` (F) and the conversion term rule (E).

Estimate 800–1,300 source lines and 500–800 unit-test lines. Checking cost
should be unchanged for programs that name no struct field in a relation; the
[result proof transport](../result-proof-transport/DESIGN.md) timing method
applies.

## Discriminating criterion

Recorded before any implementation. The extension (A–D, with E and F judged
separately) is accepted when, on a compiler implementing it and no other
language change:

1. **Probe set flips exactly as predicted.** With A–D, p1a and p1b are
   accepted and exit 9 (p1c's exit), p2a exits 24 (p2d's), p3b exits 0; with C
   alone p1g exits 3, p2f 28 and p2g 56. With C and E, p1h exits 3, and p1d
   moves from a formation refusal to FN-9 UndischargedPostcondition at its
   return (its body's `cvt.wrap` forgets the value, so the relation is false
   to prove). With C and F, p3c exits 1. p2b, p2e, p3a, p3f and p3h are still
   refused with the same rule. A refused probe that becomes accepted shows the
   change is broader than stated; an expected flip that does not happen names
   a missing transport step.
2. **Conformance.** Every new positive case runs, every new negative case is
   refused citing its rule, and every existing case keeps its verdict except
   the one named above.
3. **Snowghost.** Case 1: `atom_intern` states its Ok index bound and the
   table's growth, and a caller indexes `table.spans` with a returned atom,
   after a further intern, without a branch; `create_node` publishes its Ok
   bound, and appending a node the tree builder has just created needs no
   `InvalidNode` arm (identifiers read back from `Links` keep their check).
   Case 2: `decode_png` drops its width and height range checks, both zero
   checks and the bit-depth range checks (six of the seven guards at
   `png.wf:34-61`), with `parse_header`'s `Ok` payload and `parse_chunks`'s
   nested `header` field publishing those bounds; the area check may stay.
   `parse_header` may establish the per-dimension bounds from its area check
   by a written certificate or check them once while parsing (today they
   follow from `width * height <= 2^26` only nonlinearly, and the bit depth
   only through `depth_allowed`'s Bool). Restructuring `parse_chunks` to carry
   scalars (p2e) or to start from a valid default header is allowed. Case 3 is not a criterion for this change:
   its guards come off when the lockstep-join finding is resolved, since the
   postcondition it needs is admitted today.
4. **Cost.** Checker time on the conformance corpus and the Snowghost modules
   stays within run-to-run noise for programs that name no struct field in a
   relation.

## Where the decision goes

If selected, the ruling extends the checks-and-proofs node's decision on
measures reached from a result place, through a `design/amendments/` entry
under the design-tree procedure, and lands with the specification amendment
and its conformance cases. The lockstep-join finding stays with its
`docs/todo.md` item until its own investigation.
