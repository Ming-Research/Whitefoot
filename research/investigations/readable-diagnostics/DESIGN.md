# Readable diagnostics

## Question and requirements

`whitefootc` printed every stop as `Stage/Category [RULE]: ` followed by the
stage payload's Rust `Debug` text and, for most source stages, `at file:line:col
in line "..."`. The owner directed that diagnostics become readable, primarily
for AI agents, which write most Whitefoot code and read the output in a
compile-fix loop, and secondarily for human reviewers.

[DIAG-1](../../../spec/kernel-spec.md) fixes the content of a source
rejection: exactly one numbered rule and exactly one location from a closed
sum, plus the payload each owning rule names (FN-8's callee, requirement
clause, instantiated goal and disposition, and so on). Rendering is the
compiler's choice. The requirements selected for this work:

1. Every record states the rule, a short kind name, the location, the
   offending source line with a marker under the span, and every payload
   field under a stable label.
2. A node the payload names (FN-8's `requires_clause`, FN-9's postcondition
   and selector, a semantic capability stop's node) is printed as a position
   with its own text, not as child ordinals. `SourceId`, `NodePath`
   components and byte offsets do not appear in the default output.
3. Every family the driver prints is covered: lexical, terminal, grammar,
   canonical form, resolution, semantic, capability stops, compiler-facing
   stops, and the driver's own invocation, output and toolchain stops.
4. Output is deterministic and byte-stable, plain ASCII apart from quoted
   source text, with no color; a quoted line cannot read differently from
   its bytes.
5. Acceptance, cited rules, locations and conformance verdicts do not change.

### Evidence that motivated it

The [blind-writer report](../../experiments/blind-writer/2026-08-28/REPORT.md#63-diagnostics-scorecard)
scored six rejections: the three that carried a `kind:` payload told the
writer the offending construct and the fix (one "fixed in seconds"); the
three that carried only a coordinate "are the ones that cost time", and the
writer used `head -c` to decode `ByteOffset(11951)`. The driver since printed
the source line, but the payload stayed a `Debug` dump in which the
actionable text (residual, goal, repair) sits between internal identities. A
real OP-4 rejection before this change:

```
whitefootc: Semantics/Source [OP-4]: SemanticIssue { rule: Op4, location: SourceNode(NodePath { components: [0, 0, 7, 0, 2, 0, 1, 0, 0, 1] }, SyntaxCoordinate { source: SourceId(0), start: ByteOffset(377), end: ByteOffset(383) }), kind: UndischargedBoundsObligation { residual: "kept < deref(out).len", mechanical_fix: "when the relation must hold, ..." } } at ex1_bounds_noinv.wf:11:21 in line "      set deref(out)[kept] = byte;"
```

The same rejection after it:

```
ex1_bounds_noinv.wf:11:21: error[OP-4]: UndischargedBoundsObligation
  source:       set deref(out)[kept] = byte;
  marker:                     ^^^^^^
  residual: kept < deref(out).len
  mechanical_fix: when the relation must hold, ...
```

An FN-8 rejection names its callee's clause as a position with the clause's
own text; the old record printed
`requires_clause: NodePath { components: [0, 0, 3, 0] }`:

```
  concrete_callee: drop_spaces
  requires_clause: ex1_caller_short.wf:2:3 "requires deref(out).len >= deref(src).len;"
  instantiated_goal: buffer[0..5].len >= text[0..6].len
  disposition: Refuted
```

## The record

Every stop is one record: an envelope, a location, then the payload fields.

| Field | Present for | Text form | JSON form |
|---|---|---|---|
| `rule` | source rejections | in the summary line, `error[OP-4]` | string |
| `kind` | every compilation stop | in the summary line: the payload variant name | string |
| `category` | every stop | in the summary line for stops that are not source rejections, as words | `Source`, `Unsupported`, `Resource`, `Invocation`, `Compiler`, `Lowering`, `TargetLayout`, `Backend`; driver stops add `Output`, `Toolchain` |
| `stage` | every stop | in the summary line for stops that are not source rejections | the pipeline stage, or `Driver` |
| `at` | located stops | the summary line's `file:line:column` | `{"file","line","column"}` |
| `bytes` | located stops | omitted | `{"start","end"}`, half-open |
| `source` | located stops | `source:` line, the whole line holding the anchor | string |
| `marker` | located stops, text only | carets under the coordinate's part of that line | omitted: it re-renders `at` and `bytes` |
| payload fields | per kind | `label: value`, one per line | members of `detail` |

The text form is lean. Its summary line carries everything in the envelope a
writer acts on: `file:line:column: error[RULE]: Kind` for a source rejection,
and `file:line:column: unsupported capability in Semantics: Kind` or
`whitefootc: compiler failure in Semantics: Kind` for other stops, whose stage
is where the capability, limit or defect lives. Repeating the rule, kind,
position, category and stage as labeled lines, and printing the byte
interval, doubled the record without giving a writer anything to act on: an
earlier layout that did so was larger than the JSON object. The JSON form is
complete, carrying every envelope field and the byte interval for tools.

Measured on the two records above, the lean text is 448 and 756 bytes, the
JSON objects 540 and 869 bytes (13-17% larger), and the old `Debug` records
644 and 894 bytes. The `mechanical_fix` sentence dominates every form. The
sources are the renderer tests' `BOUNDS` program and FN-8 caller program in
`compiler/src/driver/diagnostic/tests.rs`, compiled as
`ex1_bounds_noinv.wf` and `ex1_caller_short.wf`, so the figures reproduce
with `whitefootc --emit-llvm -o /dev/null FILE 2>&1 | wc -c`, adding
`--diagnostic-format json` for the objects.

Values are typed by how a reader must read them:

- Prose, names and rendered relations are printed as written
  (`residual: kept < deref(out).len`), with invisible characters escaped so one
  field stays one line.
- Exact source bytes whose spacing or escapes matter are quoted, with every
  byte outside printable ASCII escaped: FORM-2 trivia (`expected: "\n  "`),
  the token a lexical or grammar rejection found (`found: "\u{2192}"`,
  `found: "\xff"`), and fixed terminals in an expected set. A token class in
  an expected set is named instead of quoted, so `[TYPEID, "(", literal]`
  distinguishes the class `literal` from a fixed terminal.
- A position the payload names prints as `file:line:column "text"`: the
  position is where the payload's coordinate points, and the text is the
  first line of the node that coordinate belongs to, trimmed. FN-9's selector
  prints `"result: i32"`, not the `fn` header it sits on. A declaration origin
  points at the declared name and quotes its declaration, e.g.
  `origin: a.wf:2:7 "let permit = 1_u64;"`, so the record shows what the
  other declaration is rather than repeating the spelling.
- A classification prints its variant name; one with fields prints them too,
  e.g. `obligation: Premise {use_index: 0}`.
- Lists print as `[a, b]`; structured list items as `{label: value, ...}`.

A JSON string carries exactly the characters the text form prints between
its quotes: an exact value is the same escaped spelling in both forms, so
`"\xff"` in text is `"\\xff"` in JSON, never a replacement character. A
related position is an object with `at`, `bytes` and the node's `text`; a
classification with fields is an object whose `kind` names the variant.

The `source:` line is shown as written, except that control characters,
zero-width characters (U+200B-U+200D) and the byte-order mark (U+FEFF), the
line and paragraph separators (U+2028, U+2029), bidirectional formatting
characters, and bytes that begin no valid scalar are escaped, so a quoted
line cannot display differently from its bytes. The
column counts characters, a byte that begins no scalar counting as one, and
the marker is drawn over the printed line; `bytes` in JSON is the exact
interval. Source is ASCII up to its first defect [DIAG-1], so a multi-byte
scalar on a quoted line is the defect itself or lies after it.

A compiler-supplied prelude declaration exists in no file a writer can open,
so its positions print as `<prelude>/open_file.wf:3:3` with the clause text;
the angle brackets keep an agent from opening a path that is not there.

Stops that are not source rejections carry compiler-facing payloads with no
writer repair (limits, internal invariants, target layout, backend). They
keep one `payload` field holding the stage value's `Debug` text, under a kind
named by its leading identifier. The driver's own stops (unreadable source,
invalid option, unwritable output, host toolchain) keep their one-sentence
text form and gain the common envelope in JSON:
`{"category":"Invocation","stage":"Driver","detail":{"message":"..."}}`.

## Default format: text, with JSON on request

`--diagnostic-format text|json` selects the rendering; text is the default,
provisionally.

Selection grounds, for an agent-first reader:

- The primary consumer is a language model reading stderr, not a program.
  It reads a labeled text block directly, and the text keeps source lines and
  prose unescaped; JSON escapes quotes and control bytes inside the text the
  model must compare with its source, and the lean text is smaller.
- The summary line reuses the `file:line:col: error[CODE]: ...` shape that
  compiler output throughout agent training and tooling already uses, so the
  location and rule are recognized without a format description.
- A marker under the source line only exists in text.
- Tools that must parse exactly -- a harness counting rules, a fuzz oracle, a
  writer-trial scorer -- get the complete record in one JSON object per line.
  Text lists cannot always be split unambiguously (a list item may contain
  `, `); JSON can.

These grounds are reading cost and familiarity, not a measured repair loop.
The [todo](../../../docs/todo.md) holds the writer trial that decides the
default and the marker form: compile rounds to a fix over a fixed set of
rejections. The flag keeps both forms available without a second record.

## One general rendering path

The payload types are Rust enums and structs with no reflection. Each lists
its fields once, in the diagnostic module, by exhaustive destructuring: a
`report_variants!` macro expands `Variant { a, b }` to a match arm with no
wildcard and no `..`, adds fields `a` and `b` under their own names, and
returns the variant name as the kind; the issue structures themselves
(`SemanticIssue`, `ResolutionIssue`, `SourceIssue`, `TerminalIssue`,
`SyntaxIssue`, `CanonicalIssue`, `SemanticUnsupported`, the two detail
structures and `DeclarationConflict`) are destructured field by field, their
envelope fields bound to `_` explicitly. Every closed classification a
payload names lists its variants in a `variant_names!` match with unit
patterns. A field or variant added to a payload therefore fails to compile
until it is listed, and a label cannot differ from its field name. One value
trait maps each field type to a typed value, one text renderer and one JSON
renderer read every record, and `NodePath` implements no value conversion, so
a bare path in a payload does not compile into output.

Related nodes are resolved where the tree exists. The semantic checker
already builds the rejection location from the tree; the FN-8 `requires_clause`,
the FN-9 postcondition and selector, and a semantic capability stop's node
now carry `SemanticLocation::SourceNode(path, coordinate)` instead of a bare
path, and the driver resolves every coordinate against the source bundle it
already holds. The path stays in the payload [DIAG-1]; only its position and
text are printed.

### Alternatives

- JSON by default: rejected because it escapes the source lines and prose the
  agent compares with its program, drops the marker, and is larger than the
  lean text.
- Labeled envelope lines under the summary line (`rule:`, `kind:`,
  `category:`, `stage:`, `at:`, `bytes:`): rejected because they repeat the
  summary line or give a writer nothing to act on, and made the text larger
  than JSON; JSON keeps them for tools.
- Parse each payload's derived `Debug` output into a tree and render that
  generically: rejected because `Debug` is not a stable format, hand-written
  `Debug` implementations already diverge from the derived shape
  (`ExpectedTerminals`, `SyntaxIssue`, `SourceSpan`), escaping would need a
  round trip, and a parsed `NodePath` still cannot be resolved without the tree.
- Derive a serialization trait with an external crate: rejected because the
  compiler is one crate with no dependencies built `--offline --locked`, and a
  proc-macro dependency set is a larger change than the field listing it saves.
- Hand-written prose per kind, in rustc's style: rejected for now because
  about a hundred kinds would each need a maintained sentence while the payload
  fields and the rule's `mechanical_fix` already carry the content; agents act
  on the exact fields. A per-kind headline could be added later over the same
  record.
- Wrap each payload enum definition in a macro that also generates the field
  listing: rejected because it moves roughly five hundred lines of documented
  type definitions inside a macro invocation that rustfmt and tooling do not
  format, while the destructuring listing gets the same compile-time coverage.
- Keep the resolved tree alive in the driver after a semantic rejection, or
  re-derive it on the failure path: rejected because every `SemanticOutcome`
  failure variant would have to carry the unit back (thirty-one test patterns
  bind it exactly) or the driver would repeat parsing, while the checker
  already holds the tree at the four construction sites.

## Conformance and test impact

The conformance adapter reads `CompilationFailure::kind()` and `rule_id()`;
its verdict comparison ignores the rendered text, which it keeps only as a
failure note. No conformance case, manifest, runner, adapter or wiring
changes. Compiler tests that pinned the old `Debug` text were migrated to the
record with the same strength: exact equality stays exact, each pinned
sentence is pinned as its complete `label: value` line, and each pinned kind
as the end of the summary line. The renderer's own tests pin whole records,
in both formats where the formats differ, for OP-4, FN-8 with its related
clause, a prelude requirement, FN-9 with its clause and selector, INV-1, a
certificate part with fields, a grammar rejection, a canonical-form
rejection, a multi-byte defect, an invalid byte, a line with control and
bidirectional characters, a capability stop, a compiler-facing stop, and a
driver stop.

Two explicitly invoked research harnesses read the old text: the
result-proof-transport probe matched the `whitefootc: Semantics/Source [RULE]:`
prefix and now reads the JSON `rule` and `stage`, and one container
experiment grepped two `Debug`-quoted payload fields and now greps their
unquoted lines. The differential fuzz oracle takes the first bracketed rule,
which the summary line still leads with.

## Limitations

- One rejection per compilation: the pipeline stops at its first violation,
  so an agent meets independent defects one compile at a time.
  Tracked in the [todo](../../../docs/todo.md).
- Compiler-facing stops keep a `Debug` payload; they have no writer repair
  and no current consumer that needs their fields separately (declined in the
  todo).
- A payload string the checker renders can itself hold internal spellings;
  one FN-8 goal over a float constant prints the constant's `Debug`
  (`Float { ty: F64, bits: ... }`). Tracked in the todo.
