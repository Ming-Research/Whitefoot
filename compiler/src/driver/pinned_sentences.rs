//! Every diagnostic sentence this work added, pinned by a probe that compiles
//! a source program and compares the exact rendered text.
//!
//! A diagnostic sentence is the product: it is what a writer reads and acts
//! on. A sentence no test compares is free to drift, and the gate verification
//! of 2026-08-28 found fifty-four of them in exactly that state — landed,
//! rendered, and not pinned by a test. This module is the one home for the corpus that
//! closes that: one minimal source per form, the rule it must cite, and the
//! exact fragments its rendered rejection must contain. Adding a sentence to
//! the compiler means adding a row here, or, for a repair [DIAG-1], a pair in
//! `driver::pinned_repairs`, which pins the programs the repair produces.
//!
//! The rows are deliberately redundant with the per-item tests in
//! `driver::tests` and in `semantic::tests`: those pin one sentence beside the
//! program that motivated it and explain why the sentence says what it says,
//! and this table proves no sentence is missing from that set. A sentence
//! removed from the compiler fails here; a sentence reworded fails here.
//!
//! The sentences listed below are not rows, because no source program reaches
//! them. Each is a defensive arm behind an earlier rejection or behind a
//! spelling v0.60 removed, and the reason is checkable one by one. The list is
//! kept complete rather than counted: a count is a second record of the same
//! thing and drifts, and this one already had.
//!
//! - `parameter #{ordinal}` in `entailment::flow::render_goal_datum`, and the
//!   `[parameter #{ordinal}]` subscript step beside it, name a formal, and
//!   only *concrete* goals are rendered.
//! - `no operand in position {index} for this row` in
//!   `check::expressions::calls` needs more operands than the selected row
//!   takes; [OP-1] rejects the arity first.
//! - [EFF-1]'s non-parameter-root reason and its repair need an effect root
//!   that resolves to a value and is not a parameter; the resolver rejects
//!   every such root as an unresolved `EffectRoot` use first.
//! - The two subscript-operand sentences of `check::expressions::flat_storage`
//!   -- "a written move, which consumes rather than indexes" and "an atom that
//!   is not a place", both under the expectation "a place, which a subscript
//!   indexes" -- were reached in v0.59 through the measure-former call
//!   `len_of(...)`, whose operand was an ordinary atom. v0.60 reads a measure
//!   as the `psuffix` `P.len` [OP-15, MSR-1], and [GRAM-5] writes a subscript
//!   only below a `place`, so no source presents either operand shape.
//! - "an array, buffer, or slice place", the same module's scalar-base
//!   sentence, retires with the same former for the same reason: a measure
//!   member read on an unmeasured place is [MSR-1]'s own [TYPE-5] rejection
//!   carrying the measured types, not this one.
//!
//! Two bullets this list used to carry are gone with their sentences rather
//! than with their reachability: `check::expressions::region_spelling` and
//! `slice_of`'s "a borrow of a runtime value binding or a named const" pair no
//! longer exist anywhere in the compiler, regions and view formers having left
//! the language [REF-1, REF-4].
//!
//! C2 deletes the PAR-3 staging report and its three exclusive sentences.
//! Ordinary call diagnostics remain pinned below.

use super::{CompilationFailureKind, CompilerLimits, compile};
use crate::SourceInput;

/// One probe: a minimal source, the rule its rejection must cite, and the
/// exact fragments the rendered rejection must contain.
struct Probe {
    /// The compiled unit's name, which also names the form under test.
    name: &'static str,
    /// The complete source. Minimal on purpose: everything in it is either the
    /// form under test or an ordinary caller that inhabits its signature.
    source: &'static [u8],
    /// The numbered rule [DIAG-1] must select.
    rule: &'static str,
    /// Exact substrings of the rendered rejection.
    sentences: &'static [&'static str],
}

const PROBES: &[Probe] = &[
    // -------------------------------------------------------------------
    // [FORM-3] name slots: the lexical class a grammar position writes.
    // -------------------------------------------------------------------
    Probe {
        name: "const-name-is-not-an-ident.wf",
        source: br#"const Limit: u64 = 8_u64;

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-3",
        // The whole payload, so the field labels the record prints are pinned
        // with the sentence they carry.
        sentences: &[
            "\n  expected: [IDENT]\n  found: \"Limit\"\n  mechanical_fix: an IDENT slot admits only [FORM-3]'s IDENT `[a-z][a-z0-9_]*`, so a `const`, `fn`, parameter, `let`, field, or binder name is lowercase and is never a TYPEID `[A-Z][A-Za-z0-9]*`, a LABEL `@[a-z][a-z0-9_]*`, or an OPNAME; rename the name written here to the IDENT shape",
        ],
    },
    Probe {
        name: "struct-name-is-not-a-typeid.wf",
        source: br#"struct shape {
  seq: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "a TYPEID slot admits only [FORM-3]'s TYPEID `[A-Z][A-Za-z0-9]*`, so a struct, enum, contract, variant, or constructor name is capitalized and is never an IDENT `[a-z][a-z0-9_]*`, a LABEL `@[a-z][a-z0-9_]*`, or an OPNAME; rename the name written here to the TYPEID shape",
        ],
    },
    // Retired with the region spelling: the REGIONID name-slot probe
    // `region-parameter-is-not-a-regionid.wf` pinned the sentence a `[r]`
    // region parameter published, and v0.60 has no REGIONID lexical class,
    // no region parameter and no such sentence to pin. Its successors are
    // the three name-slot probes that remain above — IDENT, TYPEID and
    // LABEL — whose own sentences no longer list REGIONID either.
    Probe {
        name: "break-target-is-not-a-label.wf",
        source: br#"fn main() -> status: std::process::ExitStatus pure {
  loop @spin {
    break spin;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-3",
        sentences: &[
            "a LABEL slot admits only [FORM-3]'s LABEL `@[a-z][a-z0-9_]*`, so write the leading `@`; an IDENT `[a-z][a-z0-9_]*`, a TYPEID `[A-Z][A-Za-z0-9]*`, and an OPNAME are other lexical classes and none is admitted here",
        ],
    },
    // -------------------------------------------------------------------
    // [GRAM-2] and [GRAM-9]: the two repairs a grammar position fixes.
    // -------------------------------------------------------------------
    // The section-order repair is pinned by a requirement written after a
    // postcondition rather than by a definition written after a requirement.
    // Both are the same [GRAM-2] mistake and the sentence names both, but the
    // frontier a `define` leaves after the [MSR-5] clause production is one
    // whose expectation list carries IDENT, and [FORM-3]'s reserved-word row
    // owns it before [GRAM-2]'s production repair is reached. That selection
    // is recorded for the owner in the batch report; the sentence itself
    // stays pinned here.
    Probe {
        name: "requires-written-after-ensures.wf",
        source: br#"fn count(end: u64) -> lines: u64 pure contract {
  ensures lines <= 8_u64;
  requires end <= 8_u64;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-2",
        sentences: &[
            "a `contract_block` is written in one fixed order: all `define` definitions first, then all `requires` requirements, then all `ensures` postconditions. A clause of an earlier section written after a later one is not admitted, so move it above the first clause of the later section",
        ],
    },
    Probe {
        name: "forbidden-atom-in-a-body.wf",
        source: br#"fn double(value: u64) -> out: u64 pure {
  return value +wrap value;
}

fn helper(value: u64) -> out: u64 pure {
  let a = double(value: double(value: value));
  return a;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-9",
        sentences: &[
            "a `call` or `construct` in an atom position does not derive [GRAM-9]: bind the inner call with its own preceding `let` in this body and write that binder in the atom position — `let inner = f(x: 0_u64); let outer = g(y: inner);`",
        ],
    },
    Probe {
        name: "forbidden-atom-in-a-contract-block.wf",
        source: br#"fn count(data: &[u8], start: u64, end: u64) -> lines: u64 reads(data) contract {
  requires imax(start, imin(start, end)) <= end;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-9",
        sentences: &[
            "a `call` or `construct` in an atom position does not derive [GRAM-9]: a `contract_block` has no `let`, so bind the inner call with a preceding `define` in this same block and write that binder in the atom position — `define inner = f(x: 0_u64); requires g(y: inner);`",
        ],
    },
    // -------------------------------------------------------------------
    // [MSR-3] and [CALL-6]: the two judgments the fact machinery adds.
    // -------------------------------------------------------------------
    Probe {
        name: "entry-of-a-shared-parameter.wf",
        source: br#"fn record(destination: &[u8]) -> written: u64 reads(destination) contract {
  ensures written == deref(entry(destination)).len;
} {
  return deref(destination).len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "MSR-3",
        sentences: &[
            "InvalidEntryFormer",
            "entry",
        ],
    },
    Probe {
        name: "contradictory-published-relations.wf",
        source: br#"fn measure(taken: Array<u8, 4>) -> measured: u64 pure contract {
  ensures measured <= taken.len;
  ensures taken.len < measured;
} {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "CALL-6",
        sentences: &[
            "ContradictoryPublishedRelations",
            "state one consistent relation set: a contract whose clauses cannot hold together publishes every fact at every caller",
        ],
    },
    // -------------------------------------------------------------------
    // [TYPE-6]: the four colliding situations.
    // -------------------------------------------------------------------
    Probe {
        name: "collides-with-a-prelude-declaration.wf",
        source: br#"struct Option {
  seq: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-6",
        sentences: &[
            "a source declaration never displaces, overrides, or shadows a PRE-1 prelude declaration of the same spelling and domain, and neither declaration resolves after the collision; rename this declaration",
        ],
    },
    Probe {
        name: "collides-with-a-prelude-opaque-declaration.wf",
        source: br#"struct Slots {
  seq: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-6",
        sentences: &[
            "a source declaration never displaces, overrides, or shadows a PRE-1 prelude declaration of the same spelling and domain, and neither declaration resolves after the collision; rename this declaration",
        ],
    },
    Probe {
        name: "redeclared-in-one-scope.wf",
        source: br#"fn main() -> status: std::process::ExitStatus pure {
  let count = 1_u64;
  let count = 2_u64;
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-6",
        sentences: &[
            "one scope declares each spelling once in a domain, so this is a redeclaration and not a shadow; rename this declaration, or delete the earlier one when nothing reads it",
        ],
    },
    Probe {
        name: "shadows-a-consumed-binding.wf",
        source: br#"struct Ticket {
  seq: u64;
}

fn consume(ticket: Ticket) -> seq: u64 pure {
  return ticket.seq;
}

fn main() -> status: std::process::ExitStatus pure {
  let permit = Ticket(seq: 1_u64);
  let used = consume(ticket: move permit);
  if used == 1_u64 {
    let permit = Ticket(seq: 2_u64);
    let again = consume(ticket: move permit);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-6",
        sentences: &[
            "]: DeclarationCollision\n",
            "\n  spelling: permit\n",
            "a declaration's scope ends with the block that declares it, and not where its value is consumed: a binding whose value was moved is dead as a value while its declaration stays live, so an inner declaration of the same spelling still collides with it. Rename the inner declaration, or close the block that declares the outer one before this point",
        ],
    },
    // -------------------------------------------------------------------
    // [FN-8, CALL-1] and [OP-4]: the residual names the caller's place.
    // -------------------------------------------------------------------
    Probe {
        name: "prelude-range-residual.wf",
        source: br#"fn main(out: std::io::OutputStream, factory: std::io::HandleFactory) -> status: std::process::ExitStatus pure {
  let header = array_filled::<u8, 4>(value: 65_u8);
  let payload = array_filled::<u8, 9>(value: 66_u8);
  let wide = payload.len;
  let view = &header[0_u64..4_u64];
  let sent = std::io::write_once(factory: &factory, output: &out, source: view, start: 0_u64, end: wide);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        // Regression for the range-root substitution: the goal must retain
        // the range holder's own `len` rather than becoming `header.len`
        // (ref4-neg-a-requirement-over-a-range-reference-is-the-ranges-length).
        sentences: &["\n  instantiated_goal: wide <= deref(view).len\n"],
    },
    Probe {
        name: "bounds-residual.wf",
        source: br#"fn main() -> status: std::process::ExitStatus pure {
  let table = array_filled::<u8, 4>(value: 0_u8);
  let other = array_filled::<u8, 9>(value: 0_u8);
  let pick = other.len;
  let one = table[pick];
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OP-4",
        sentences: &["\n  residual: pick < table.len\n"],
    },
    // -------------------------------------------------------------------
    // [FN-2]: written type and region arguments.
    // -------------------------------------------------------------------
    // C2 removes SYS-2's separate region-argument diagnostic. Ordinary
    // FN-2 arity and FORM-8 call-region probes below retain their coverage.
    // The two region-arity rows this table carried until v0.42 are retired
    // with the sentences they pinned: [FORM-8] gives a call exactly one legal
    // region-argument list, so "no region argument list" and "too many region
    // arguments" are no longer faults a call can commit. Their replacements
    // are the two FORM-8 rows at the end of this table. The generic half of
    // FN-2's arity sentence survives and is pinned here.
    Probe {
        name: "call-without-its-type-arguments.wf",
        source: br#"fn identity<T: Int>(value: T) -> out: T pure {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let doubled = identity(value: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-2",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: 1 written generic argument\n  found: no explicit argument list\n",
        ],
    },
    // -------------------------------------------------------------------
    // [TYPE-5] generic argument lists and [FN-3] bounds.
    // -------------------------------------------------------------------
    Probe {
        name: "construct-without-its-type-arguments.wf",
        source: br#"struct Pair<T: drop> {
  left: T;
  right: T;
}

fn main() -> status: std::process::ExitStatus pure {
  let p = Pair(left: 1_u64, right: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: 1 written generic argument\n  found: no explicit argument list\n",
        ],
    },
    Probe {
        name: "construct-with-too-many-type-arguments.wf",
        source: br#"struct Pair<T: drop> {
  left: T;
  right: T;
}

fn main() -> status: std::process::ExitStatus pure {
  let p = Pair<u64, u64>(left: 1_u64, right: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: 1 written expanded generic argument\n  found: 2 written expanded generic arguments\n",
        ],
    },
    Probe {
        name: "const-written-in-a-type-parameter-position.wf",
        source: br#"struct Pair<T: drop> {
  left: T;
  right: T;
}

fn main() -> status: std::process::ExitStatus pure {
  let p = Pair<4>(left: 1_u64, right: 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: a type argument occupies this parameter position\n  found: a nonmatching behavior argument\n",
        ],
    },
    Probe {
        name: "type-written-in-a-const-parameter-position.wf",
        source: br#"struct Row<const n: u64> {
  count: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = Row<u64>(count: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: a const argument occupies this parameter position\n  found: a nonmatching behavior argument\n",
        ],
    },
    Probe {
        name: "type-arguments-on-a-form-that-declares-none.wf",
        source: br#"struct Plain {
  value: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let p = Plain<u64>(value: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: 0 written expanded generic arguments\n  found: 1 written expanded generic argument\n",
        ],
    },
    Probe {
        name: "type-arguments-on-a-type-that-takes-none.wf",
        source: br#"fn take(value: Bool<u8>) -> out: u64 pure {
  return 0_u64;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: this type spelled with no type arguments\n  found: a written `<...>` type-argument list on a type that takes none\n",
        ],
    },
    Probe {
        name: "int-bound-is-not-satisfied.wf",
        source: br#"fn widen<T: Int>(value: T) -> out: T pure {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = widen::<f64>(value: 1.0_f64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-3",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: an integer type, which the parameter's `Int` bound requires\n  found: f64\n",
        ],
    },
    Probe {
        name: "float-bound-is-not-satisfied.wf",
        source: br#"fn scale<T: Float>(value: T) -> out: T pure {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let a = scale::<u64>(value: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-3",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: a float type, which the parameter's `Float` bound requires\n  found: u64\n",
        ],
    },
    // -------------------------------------------------------------------
    // [TYPE-5] Result and Option: both spellings that carry the arguments.
    // -------------------------------------------------------------------
    Probe {
        name: "result-without-type-arguments.wf",
        source: br#"fn pick(value: u64) -> out: Result pure {
  return Ok(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: Result with both type arguments written: as a type `Result<u64, IoError>`, and as a variant constructor `Ok<u64, IoError>(value: v)`\n  found: Result with no written type-argument list\n",
        ],
    },
    Probe {
        name: "result-with-one-type-argument.wf",
        source: br#"fn pick(value: u64) -> out: Result<u64> pure {
  return Ok<u64>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: Result<T, E> with exactly two type arguments\n  found: a Result type-argument list of a different length\n",
        ],
    },
    Probe {
        name: "result-with-a-const-type-argument.wf",
        source: br#"fn pick(value: u64) -> out: Result<4, std::io::IoError> pure {
  return Ok<4, std::io::IoError>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: a type in each Result type-argument position\n  found: a const argument in a Result type-argument position\n",
        ],
    },
    Probe {
        name: "option-without-type-arguments.wf",
        source: br#"fn pick(value: u64) -> out: Option pure {
  return Some(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: Option with its type argument written: as a type `Option<u64>`, and as a variant constructor `Some<u64>(value: v)`\n  found: Option with no written type-argument list\n",
        ],
    },
    Probe {
        name: "option-with-two-type-arguments.wf",
        source: br#"fn pick(value: u64) -> out: Option<u64, u64> pure {
  return Some<u64, u64>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: Option<T> with exactly one type argument\n  found: an Option type-argument list of a different length\n",
        ],
    },
    Probe {
        name: "option-with-a-const-type-argument.wf",
        source: br#"fn pick(value: u64) -> out: Option<4> pure {
  return Some<4>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: a type in the Option type-argument position\n  found: a const argument in the Option type-argument position\n",
        ],
    },
    // Retired with the measure-former call spelling. v0.59 read a measure by
    // calling `len_of(P)`, and these three probes pinned that call's own
    // operand sentences (`indexed-operand-is-a-move`,
    // `indexed-operand-is-not-a-place`, `indexed-place-is-a-scalar`). v0.60
    // reads a measure as the `psuffix` `P.len` [OP-15, MSR-1], so no source
    // reaches those sentences; they join the unreachable list above. The
    // measure place's own defects are pinned by the [OP-4] and [INV-1] probes.
    // Retired with the view kinds it named. `Slice<T>`, arenas and providers
    // are not types in v0.60, and [GRAM-3]'s `type` has no reference
    // production, so no written source reaches [STOR-5] at all: the rule is
    // now the closing clause for a substituted generic instance alone. Its
    // sentence joins the unreachable list in this module's header.
    // -------------------------------------------------------------------
    // [EFF-1] and [EFF-2]: the declared row.
    // -------------------------------------------------------------------
    // Retired with the permission marker. v0.60 has no `&uniq`, so a
    // `writes` path rooted at a reference parameter is the ordinary spelling
    // [REF-1, EFF-1] and there is no shared-versus-unique defect left to
    // publish. The successors are the two [EFF-1] row defects pinned below.
    Probe {
        // Renamed from `repeated-effect-category.wf`: the source repeats one
        // *path* within one category, `reads(left), reads(left)`, which is
        // exactly what the pinned sentence says a row may not do. Nothing here
        // repeats a category.
        name: "repeated-effect-path.wf",
        source: br#"fn touch(left: &u64, right: &u64) -> out: u64 reads(left), reads(left), reads(right) {
  let a = deref(left);
  let b = deref(right);
  return a +wrap b;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-1",
        sentences: &[
            "]: InvalidEffectRow\n",
            "\n  reason: a row lists each path at most once per category, and this entry repeats one\n  mechanical_fix: delete the repeated entry\n",
        ],
    },
    Probe {
        name: "effect-suffix-on-a-non-struct.wf",
        source: br#"fn touch(value: &u64) -> out: u64 reads(value.count) {
  return deref(value);
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-1",
        sentences: &[
            "]: InvalidEffectRow\n",
            "\n  reason: each effect-path suffix must select a field, payload, measure, window part, or indexed position admitted by its prefix type\n  mechanical_fix: select a member or position admitted by the prefix type, or name the reference parameter's complete state; use .inner for Box contents\n",
        ],
    },
    Probe {
        name: "effect-suffix-names-an-undeclared-field.wf",
        source: br#"struct Pair {
  left: u64;
  right: u64;
}

fn touch(pair: &Pair) -> out: u64 reads(pair.middle) {
  return deref(pair).left;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-1",
        sentences: &[
            "]: InvalidEffectRow\n",
            "\n  reason: an effect-path suffix names a member its selected type does not declare\n  mechanical_fix: name a declared member of that type, or the reference parameter itself\n",
        ],
    },
    Probe {
        name: "declared-row-is-narrower-than-the-body.wf",
        source: br#"fn touch(data: &[u8]) -> out: u64 pure {
  return deref(data).len;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "]: EffectMismatch\n",
            "\n  expected_row: reads(data.len)\n  found_row: pure\n  missing: [reads(data.len)]\n  extra: []\n  mechanical_fix: declare the row as `reads(data.len)`, which covers every access the body makes and no other\n",
        ],
    },
    Probe {
        // The body reads and writes `stats.count` and never touches
        // `stats.total`: the suggestion drops the read the write subsumes
        // [EFF-1], nothing is missing, and the untouched write is extra.
        name: "declared-row-writes-an-unexhibited-field.wf",
        source: br#"struct Stats {
  count: u64;
  total: u64;
}

fn record(stats: &Stats) -> result: unit writes(stats.count), writes(stats.total) {
  let old = deref(stats).count;
  set deref(stats).count = old +wrap 1_u64;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-2",
        sentences: &[
            "\n  expected_row: writes(stats.count)\n  found_row: writes(stats.count), writes(stats.total)\n  missing: []\n  extra: [writes(stats.total)]\n",
        ],
    },
    Probe {
        name: "read-subsumed-by-a-write-of-the-same-path.wf",
        source: br#"fn bump(value: &u64) -> out: u64 reads(value), writes(value) {
  let old = deref(value);
  set deref(value) = old +wrap 1_u64;
  return old;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-1",
        sentences: &[
            "]: SubsumedEffectEntry\n",
            "\n  entry: reads(value)\n  covering: writes(value)\n",
        ],
    },
    Probe {
        name: "write-below-a-written-path.wf",
        source: br#"struct Pair {
  first: u8;
  second: u8;
}

fn reset(pair: &Pair) -> result: unit writes(pair), writes(pair.first) {
  set deref(pair) = Pair(first: 0_u8, second: 0_u8);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-1",
        sentences: &[
            "]: SubsumedEffectEntry\n",
            "\n  entry: writes(pair.first)\n  covering: writes(pair)\n",
        ],
    },
    Probe {
        name: "read-below-a-read-path.wf",
        source: br#"struct Pair {
  first: u8;
  second: u8;
}

fn inspect(pair: &Pair) -> result: u8 reads(pair), reads(pair.first) {
  let whole = deref(pair);
  return whole.second;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "EFF-1",
        sentences: &[
            "]: SubsumedEffectEntry\n",
            "\n  entry: reads(pair.first)\n  covering: reads(pair)\n",
        ],
    },
    // -------------------------------------------------------------------
    // [TYPE-5] places, subscripts, and flat storage.
    // -------------------------------------------------------------------
    Probe {
        name: "buffer-length-is-not-a-u64.wf",
        source: br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = 1_u64 > 0_u64;
  let store = box_array_filled::<u8>(count: flag, value: 0_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &["]: TypeMismatch\n", "\n  expected: u64\n  found: Bool\n"],
    },
    Probe {
        // Field suffixes after indices are supported; this scalar element
        // still has no fields. Pin that type rule, not the retired path limit.
        name: "scalar-buffer-element-has-no-fields.wf",
        source: br#"fn main() -> status: std::process::ExitStatus pure {
  let store = array_filled::<u8, 4>(value: 0_u8);
  let one = store[0_u64].value;
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: a source struct, whose declared field this suffix selects\n  found: u8\n",
        ],
    },
    // -------------------------------------------------------------------
    // [OWN-4], [OWN-6], and [OWN-10]: regions and reborrows.
    // -------------------------------------------------------------------
    // The former two-statement source is now an acceptance witness in the
    // semantic borrow tests. Pin the retained local-region condition here.
    // Retired with `slice_of` and the permission marker: the four probes
    // `slice-of-a-non-borrow`, `slice-of-a-unique-borrow`,
    // `borrow-kind-does-not-match-the-destination` and
    // `shared-borrow-where-a-unique-one-is-required` each pinned a sentence
    // about a borrow kind or a view former, and v0.60 has neither [REF-1,
    // REF-4]. The general [TYPE-5] mismatch sentence they shared stays
    // pinned by `buffer-length-is-not-a-u64` and
    // `slice-value-where-a-scalar-is-required` below.
    Probe {
        name: "slice-value-where-a-scalar-is-required.wf",
        source: br#"const digits: Array<u8, 2> =[48_u8, 49_u8];

fn measure(view: u64) -> out: u64 pure {
  return view;
}

fn main() -> status: std::process::ExitStatus pure {
  let view = &digits[0_u64..2_u64];
  let n = measure(view: view);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        // The actual here is the range reference `view`, whose kind is
        // `&[u8]`: [TYPE-8] states that "`&T` and `&[T]` are reference kinds
        // and not types", and [REF-4] gives the formation `&digits[0..2]`
        // exactly that kind. The element type `u8` is the type of
        // `deref(view)[i]` and of nothing at this argument position, so a
        // payload naming it describes a value the program does not contain.
        //
        // [TYPE-5] fixes that "argument types match declared parameter types
        // exactly" and therefore that this call is rejected; it does not fix
        // the rendering of a call-argument mismatch, and [DIAG-3] requires
        // byte identity "only where this specification explicitly fixes both
        // selection and encoding". Pinned here are the two facts the
        // specification does fix, inside the `expected`/`found` framing every
        // other TYPE-5 probe in this module shares: the declared parameter's
        // mode and type, written the way [TYPE-5] writes a `set` target's
        // ("carrying expected `own T` and the actual mode and type") because
        // [FN-1] makes a parameter a mode and a type together; and the actual,
        // named as the range reference it is. The actual carries no mode
        // because a reference kind has none.
        sentences: &[
            "\n  expected: own u64\n",
            "\n  found: &[u8]\n",
        ],
    },
    // -------------------------------------------------------------------
    // [TYPE-5] and [FORM-5]: projections, replacement, and operands.
    // -------------------------------------------------------------------
    Probe {
        name: "projection-of-a-non-struct.wf",
        source: br#"fn peek(value: u64) -> out: u64 pure {
  return value.count;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: a source struct, whose declared field this suffix selects\n  found: u64\n",
        ],
    },
    Probe {
        name: "projection-of-an-undeclared-field.wf",
        source: br#"struct Pair {
  left: u64;
  right: u64;
}

fn peek(pair: Pair) -> out: u64 pure {
  return pair.middle;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: a declared field of Pair\n  found: the field name `middle`, which that struct does not declare\n",
        ],
    },
    Probe {
        name: "replacement-value-has-another-type.wf",
        source: br#"struct Ticket {
  seq: u64;
}

fn main() -> status: std::process::ExitStatus pure {
  let ticket = Ticket(seq: 1_u64);
  set ticket = 2_u64;
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        // [TYPE-5] fixes both halves of this payload: "the right-hand side of
        // `set p = e;` must produce exactly `own T`", and "a different
        // right-hand-side mode or type is a hard error citing TYPE-5 at the
        // complete `expr` child of the `set_stmt`, carrying expected `own T`
        // and the actual mode and type". A bare type on either side drops the
        // mode the rule names.
        sentences: &["]: TypeMismatch\n", "\n  expected: own Ticket\n  found: own u64\n"],
    },
    Probe {
        name: "boolean-operand-is-an-integer.wf",
        source: br#"fn main() -> status: std::process::ExitStatus pure {
  let flag = band(1_u64, 2_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &["]: TypeMismatch\n", "\n  expected: own Bool\n  found: own u64\n"],
    },
    Probe {
        name: "match-scrutinee-is-not-an-enum.wf",
        source: br#"fn main() -> status: std::process::ExitStatus pure {
  let value = 1_u64;
  match value {
    Ok(value: inner) => {
    }
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "TYPE-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: an enum scrutinee, whose variants the arms match\n  found: u64\n",
        ],
    },
    Probe {
        name: "generic-numeric-identity-of-a-non-numeric-type.wf",
        source: br#"fn zeroed<T: drop>(sample: T) -> out: T pure {
  return 0_T;
}

fn main() -> status: std::process::ExitStatus pure {
  let flag = 1_u64 > 0_u64;
  let a = zeroed::<Bool>(sample: flag);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FORM-5",
        sentences: &[
            "]: TypeMismatch\n",
            "\n  expected: an integer or float type, whose 0 and 1 this form names\n",
        ],
    },
    // -------------------------------------------------------------------
    // [FN-8]: the goal, rendered in the source terms of the caller.
    // -------------------------------------------------------------------
    Probe {
        name: "goal-with-an-infix-operation.wf",
        source: br#"fn need(x: u64) -> out: u64 pure contract {
  define bumped = x +wrap 1_u64;
  requires bumped < 10_u64;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let s = 3_u64;
  let r = need(x: s);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &["\n  instantiated_goal: s +wrap 1_u64 < 10_u64\n"],
    },
    Probe {
        name: "goal-with-a-numeric-conversion.wf",
        source: br#"fn need(x: u32) -> out: u32 pure contract {
  define wide = cvt::<u32, u64>(x);
  requires wide < 10_u64;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let bytes = array_filled::<u8, 1>(value: 3_u8);
  let raw = bytes[0_u64];
  let s = cvt::<u8, u32>(raw);
  let r = need(x: s);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        // The actual is read from a buffer element rather than written as a
        // literal: a conversion of a known constant is now discharged by the
        // affine route, which reaches a call goal that projects to no L0
        // relation, so a literal actual would prove the requirement and print
        // no diagnostic. The pinned sentence is unchanged.
        sentences: &["\n  instantiated_goal: cvt::<u32, u64>(s) < 10_u64\n"],
    },
    Probe {
        name: "goal-with-a-reinterpretation.wf",
        source: br#"fn need(x: i64) -> out: i64 pure contract {
  define raw = reinterpret::<i64, u64>(x);
  requires raw < 10_u64;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let s = 3_i64;
  let r = need(x: s);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &["\n  instantiated_goal: reinterpret::<i64, u64>(s) < 10_u64\n"],
    },
    Probe {
        name: "goal-with-a-float-literal.wf",
        source: br#"fn need(x: f64) -> out: f64 pure contract {
  requires flt(x, 1.0_f64);
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let v = 2.0_f64;
  let r = need(x: v);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &["\n  instantiated_goal: flt(v, 1.0_f64)\n"],
    },
    Probe {
        name: "goal-over-an-admitted-index-actual.wf",
        source: br#"fn need(x: u8) -> out: u8 pure contract {
  requires x < 10_u8;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let data = array_filled::<u8, 4>(value: 0_u8);
  let r = need(x: data[0_u64]);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &["\n  instantiated_goal: data[0_u64] < 10_u8\n"],
    },
    Probe {
        name: "goal-over-a-dereferenced-holder.wf",
        source: br#"fn need(names: &[u8], pos: u64) -> out: u64 pure contract {
  define spare = deref(names).len;
  requires pos <= spare;
} {
  return pos;
}

fn outer(names: &[u8]) -> out: u64 pure {
  let r = need(names: names, pos: 9_u64);
  return r;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        // [OP-15] spells a measure read through a reference `deref(names).len`;
        // the renderer currently drops the `deref`. The pinned sentence is the
        // specification spelling and stays failing until the renderer is fixed.
        sentences: &["\n  instantiated_goal: 9_u64 <= deref(names).len\n"],
    },
    Probe {
        // A generic callee is named as a call writes it [FN-2], never by the
        // symbol that keys its lowering.
        name: "goal-of-a-generic-instance.wf",
        source: br#"fn need<const n: u64>(x: u64) -> out: u64 pure contract {
  requires x < n;
} {
  return x;
}

fn main() -> status: std::process::ExitStatus pure {
  let r = need::<4>(x: 9_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-8",
        sentences: &["\n  concrete_callee: need::<4>\n"],
    },
    // -------------------------------------------------------------------
    // [FN-9]: the selected return, named by its instance.
    // -------------------------------------------------------------------
    Probe {
        name: "postcondition-of-a-generic-instance.wf",
        source: br#"fn bad<T: Int>(value: T) -> result: T pure contract {
  ensures result < value;
} {
  return value;
}

fn main() -> status: std::process::ExitStatus pure {
  let ignored = bad::<u8>(value: 0_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "FN-9",
        sentences: &["\n  concrete_function: bad::<u8>\n"],
    },
    // -------------------------------------------------------------------
    // [PROV-6] and [GRAM-8]: a generic nominal instance is named as its
    // type is written [GRAM-3], never by the key the checker interned it
    // under.
    // -------------------------------------------------------------------
    Probe {
        name: "unconsumed-instance-of-a-generic-nodrop-struct.wf",
        source: br#"nodrop struct Token<T> {
  value: T;
}

fn main() -> status: std::process::ExitStatus pure {
  let token = Token<u64>(value: 1_u64);
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "PROV-6",
        sentences: &["\n  binding: token\n  obligation: Token<u64>\n"],
    },
    Probe {
        name: "const-of-a-generic-struct-with-a-wrong-field.wf",
        source: br#"struct Wrap<T> {
  value: T;
}

const w: Wrap<u64> = Wrap<u64>(other: 1_u64);

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "GRAM-8",
        sentences: &[
            "]: InvalidConstructionFields\n",
            "\n  constructor: Wrap<u64>\n  declared_fields: [value]\n",
        ],
    },
    // [FORM-8] one canonical region spelling: each position a region can
    // occupy, written exactly where the surrounding text does not fix it.
    // [FORM-8, OWN-11] a loop body is itself a region block, so a block that
    // is the body's only statement spells that one region twice.
    // -------------------------------------------------------------------
    // [LIV-1] and [LIV-2]: join-checked liveness and the one commit.
    // -------------------------------------------------------------------
    Probe {
        name: "branches-disagree-about-a-binding.wf",
        source: br#"fn measure(cell: Box<Array<u8>>) -> size: u64 pure {
  let n = cell.inner.len;
  return n;
}

fn main() -> status: std::process::ExitStatus pure {
  let c = box_array_filled::<u8>(count: 4_u64, value: 0_u8);
  let flag = 1_u64;
  let taken = 0_u64;
  if flag == 1_u64 {
    set taken = measure(cell: move c);
  } else {
    set taken = 7_u64;
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "LIV-1",
        sentences: &[
            "\n  binding: c\n",
            "\n  live_predecessor: the `else` branch\n",
            "\n  dead_predecessor: the `if` branch\n",
            "every predecessor of a join agrees on a binding\'s live-or-dead status: consume it on every predecessor, on none, or commit a value back into it before the predecessor that consumed it reaches the join",
        ],
    },
    Probe {
        name: "one-iteration-leaves-an-outer-binding-dead.wf",
        source: br#"fn measure(cell: Box<Array<u8>>) -> size: u64 pure {
  let n = cell.inner.len;
  return n;
}

fn main() -> status: std::process::ExitStatus pure {
  let c = box_array_filled::<u8>(count: 4_u64, value: 0_u8);
  for (i in 0_u64..2_u64) {
    let taken = measure(cell: move c);
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "OWN-11",
        sentences: &[
            "\n  binding: c\n",
            "one iteration must leave every outer binding in the status the next one starts from: commit a value back into it before the backedge, or declare and consume it inside the body",
        ],
    },
    // [VIEW-4]: the same commit at a *copy* view, whose target [LIV-2]'s own
    // condition would admit with nothing consumed.
    // -------------------------------------------------------------------
    // [BLK-1] and [PROV-1]: the container nominals a construct may not name
    // and the store region an extent always writes.
    // -------------------------------------------------------------------
    // -------------------------------------------------------------------
    // [INV-1]: the one `call` an affine factor admits is a measure former.
    // -------------------------------------------------------------------
    Probe {
        name: "an-affine-factor-that-is-not-a-measure.wf",
        source: br#"fn main() -> status: std::process::ExitStatus pure {
  let limit = 4_u64;
  let seen = 0_u64;
  for (
    at in 0_u64..4_u64,
    invariant bounded: seen <= imin(limit, limit)
  ) {
  }
  return std::process::exit_status(code: 0_u8);
}
"#,
        rule: "INV-1",
        sentences: &[
            "an affine factor calls something other than a measure former",
            "write P.len, P.cap or P.head over a measured place",
        ],
    },
    // Retired with the rules whose sentences they pinned. Each probe below
    // cited a rule v0.60 does not have, so the sentence it compared no longer
    // exists anywhere in the compiler and no source can reach it:
    //   - slice-of-arena-content-under-a-foreign-region.wf [OWN-10]
    //   - borrow-of-local-storage-under-a-parameter-region.wf [OWN-10]
    //   - returned-child-reborrow-names-a-foreign-region.wf [OWN-10]
    //   - returned-borrow-of-a-local-region.wf [OWN-4]
    //   - caller-region-for-an-argument-child.wf [OWN-6]
    //   - region-written-at-an-unrelated-parameter.wf [FORM-8]
    //   - region-elided-at-a-result.wf [FORM-8]
    //   - region-parameter-list-out-of-order.wf [FORM-8]
    //   - region-written-at-the-innermost-borrow.wf [FORM-8]
    //   - region-block-is-the-whole-loop-body.wf [FORM-8]
    //   - region-block-name-nothing-references.wf [FORM-8]
    //   - region-argument-the-call-determines.wf [FORM-8]
    //   - prelude-region-argument-the-call-determines.wf [FORM-8]
    //   - two-targets-of-one-commit-overlap.wf [LIV-2]
    //   - a-commit-target-carrying-a-region.wf [LIV-2]
    //   - a-commit-displacing-a-live-loan.wf [VIEW-4]
    //   - a-construct-naming-a-run.wf [BLK-1]
    //   - a-construct-naming-a-provider.wf [BLK-1]
    //   - an-extent-eliding-its-store-region.wf [FORM-8]
    // The region and loan rules [OWN-3, OWN-4, OWN-6, OWN-10, FORM-8, VIEW-4,
    // BLK-1, LIV-2] left the language with regions, permission markers, views
    // and the fallible store take. Their successors are the reference rules
    // [REF-1, REF-3, REF-4] and the window rules [WIN-1, WIN-3, OP-10],
    // whose own sentences are pinned by the probes that remain above.
];

/// Every sentence in the corpus is rendered by a program that reaches it.
#[test]
fn every_diagnostic_sentence_is_pinned_by_a_probe() {
    for probe in PROBES {
        let failure = compile(
            &[SourceInput::new(probe.name, probe.source)],
            CompilerLimits::default(),
        )
        .expect_err(probe.name);
        assert_eq!(
            failure.kind(),
            CompilationFailureKind::Source,
            "{}: a probe rejects source, and never stops on compiler capability: {failure}",
            probe.name
        );
        assert_eq!(
            failure.rule_id(),
            Some(probe.rule),
            "{}: {failure}",
            probe.name
        );
        // The complete record `whitefootc` prints, kind name included. Every
        // field is one `\n  label: value` line; the closing newline lets a
        // probe pin a last field as a complete line too.
        let rendered = format!("{failure}\n");
        for sentence in probe.sentences {
            assert!(
                rendered.contains(sentence),
                "{}: the rendered rejection no longer carries this sentence.\nwanted: {sentence}\ngot:    {rendered}",
                probe.name,
            );
        }
    }
}
