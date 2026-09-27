//! Ordinary PRE-1 declaration records, parsed by the same grammar as source declarations.
//! An opaque record has a refused constructor [TYPE-2] and the cell `Box` has
//! one field; function signatures have no body. The host declarations are
//! the standard library's host modules [PRE-2], not prelude records.

use crate::source::PreludeSource;

pub(crate) const DECLARATIONS: &[(&str, PreludeSource, &str)] = &[
    // [PRE-1] writes the three storage shapes first, then the cell. [TYPE-2]
    // makes each of the four an opaque struct
    // with a constructor entry that exists to be refused, and [TYPE-9] keeps
    // their element storage compiler-owned: a declaration can state neither
    // the elements nor the omitted-capacity form, so what the body carries is
    // exactly the readonly measure fields [MSR-1].
    //
    // The fence writes the capacity parameter `const N: u64`. That spelling
    // does not parse: [GRAM-2]'s `gparam := "const" IDENT ":" type` takes a
    // lexical IDENT, and [TYPE-2] says so outright -- "in this
    // specification's prose `N` stands for a written const argument; source
    // writes a `const` IDENT, lowercase under [FORM-3], as the [PRE-1] rows
    // do". The rows below therefore write `const n: u64`, exactly as
    // `slots_new<T, const n: u64>` of the same fence does.
    //
    // [OWN-1] `Array` carries no capability modifier, so an instance has the
    // capabilities of its element; `Slots`, `Ring` and `Box` are `nocopy`.
    (
        "prelude/Array.wf",
        PreludeSource::Opaque,
        r#"opaque struct Array<T, const n: u64> {
  readonly len: u64;
}
"#,
    ),
    (
        "prelude/Slots.wf",
        PreludeSource::Opaque,
        r#"opaque nocopy struct Slots<T, const n: u64> {
  readonly len: u64;
  readonly cap: u64;
}
"#,
    ),
    (
        "prelude/Ring.wf",
        PreludeSource::Opaque,
        r#"opaque nocopy struct Ring<T, const n: u64> {
  readonly len: u64;
  readonly cap: u64;
  readonly head: u64;
}
"#,
    ),
    // [PRE-1] writes the cell after the three shapes. [TYPE-2] makes it an
    // opaque struct with one field and a constructor
    // entry that exists to be refused.
    //
    // [GRAM-2]'s `gparam := TYPEID (":" (TYPEID | capability_bound))?` makes
    // the bound optional and [PROV-6] reads an absent bound as no capability,
    // so `Box<T>` admits a content of every class, as `box_new<T>` of the
    // same fence does.
    (
        "prelude/Box.wf",
        PreludeSource::Opaque,
        r#"opaque nocopy struct Box<T> {
  inner: T;
}
"#,
    ),
    (
        "prelude/box_new.wf",
        PreludeSource::Function,
        r#"fn box_new<T>(value: T) -> result: Box<T> pure;
"#,
    ),
    (
        "prelude/array_filled.wf",
        PreludeSource::Function,
        r#"fn array_filled<T: copy, const n: u64>(value: T) -> result: Array<T, n> pure contract {
  ensures result.len == n;
};
"#,
    ),
    (
        "prelude/slots_new.wf",
        PreludeSource::Function,
        r#"fn slots_new<T, const n: u64>() -> result: Slots<T, n> pure contract {
  ensures result.len == 0_u64;
  ensures result.cap == n;
};
"#,
    ),
    (
        "prelude/ring_new.wf",
        PreludeSource::Function,
        r#"fn ring_new<T, const n: u64>() -> result: Ring<T, n> pure contract {
  ensures result.len == 0_u64;
  ensures result.cap == n;
  ensures result.head == 0_u64;
};
"#,
    ),
    (
        "prelude/box_array_filled.wf",
        PreludeSource::Function,
        r#"fn box_array_filled<T: copy>(count: u64, value: T) -> result: Box<Array<T>> pure contract {
  ensures result.inner.len == count;
};
"#,
    ),
    (
        "prelude/box_slots_new.wf",
        PreludeSource::Function,
        r#"fn box_slots_new<T>(capacity: u64) -> result: Box<Slots<T>> pure contract {
  ensures result.inner.len == 0_u64;
  ensures result.inner.cap == capacity;
};
"#,
    ),
    (
        "prelude/box_ring_new.wf",
        PreludeSource::Function,
        r#"fn box_ring_new<T>(capacity: u64) -> result: Box<Ring<T>> pure contract {
  ensures result.inner.len == 0_u64;
  ensures result.inner.cap == capacity;
  ensures result.inner.head == 0_u64;
};
"#,
    ),
    (
        "prelude/slots_from_array.wf",
        PreludeSource::Function,
        r#"fn slots_from_array<T, const n: u64>(values: Array<T, n>) -> result: Slots<T, n> pure contract {
  ensures result.len == n;
  ensures result.cap == n;
};
"#,
    ),
    (
        "prelude/slots_into_array.wf",
        PreludeSource::Function,
        r#"fn slots_into_array<T, const n: u64>(values: Slots<T, n>) -> result: Array<T, n> pure contract {
  requires values.len == n;
  ensures result.len == n;
};
"#,
    ),
    (
        "prelude/place_back.wf",
        PreludeSource::Function,
        r#"fn place_back<W, T>(window: &W, value: T) -> result: unit writes(window.next), writes(window.len) contract {
  requires deref(window).len < deref(window).cap;
  ensures deref(window).len == deref(entry(window)).len + 1_u64;
};
"#,
    ),
    (
        "prelude/take_back.wf",
        PreludeSource::Function,
        r#"fn take_back<W, T>(window: &W) -> value: T writes(window.last), writes(window.len) contract {
  requires deref(window).len > 0_u64;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
};
"#,
    ),
    (
        "prelude/insert_at.wf",
        PreludeSource::Function,
        r#"fn insert_at<W, T>(window: &W, index: u64, value: T) -> result: unit writes(window.filled), writes(window.next), writes(window.len) contract {
  requires index <= deref(window).len;
  requires deref(window).len < deref(window).cap;
  ensures deref(window).len == deref(entry(window)).len + 1_u64;
};
"#,
    ),
    (
        "prelude/remove_at.wf",
        PreludeSource::Function,
        r#"fn remove_at<W, T>(window: &W, index: u64) -> value: T writes(window.filled), writes(window.len) contract {
  requires index < deref(window).len;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
};
"#,
    ),
    (
        "prelude/append.wf",
        PreludeSource::Function,
        r#"fn append<W, X>(destination: &W, source: &X) -> result: unit writes(destination.free), writes(destination.len), writes(source.filled), writes(source.len) contract {
  requires deref(source).len <= deref(destination).cap - deref(destination).len;
  ensures deref(destination).len >= deref(entry(destination)).len;
  ensures deref(destination).len >= deref(entry(source)).len;
  ensures deref(source).len == 0_u64;
};
"#,
    ),
    (
        "prelude/split_off.wf",
        PreludeSource::Function,
        r#"fn split_off<W, X>(source: &W, index: u64, destination: &X) -> result: unit writes(source.filled), writes(source.len), writes(destination.free), writes(destination.len) contract {
  requires index <= deref(source).len;
  requires deref(source).len - index <= deref(destination).cap - deref(destination).len;
  ensures deref(source).len == index;
  ensures deref(destination).len >= deref(entry(destination)).len;
};
"#,
    ),
    (
        "prelude/grow.wf",
        PreludeSource::Function,
        r#"fn grow<T>(cell: &Box<Slots<T>>, capacity: u64) -> result: unit writes(cell) contract {
  requires capacity >= deref(cell).inner.cap;
  ensures deref(cell).inner.cap == capacity;
  ensures deref(cell).inner.len == deref(entry(cell)).inner.len;
};
"#,
    ),
    (
        "prelude/place_front.wf",
        PreludeSource::Function,
        r#"fn place_front<W, T>(window: &W, value: T) -> result: unit writes(window) contract {
  requires deref(window).len < deref(window).cap;
  ensures deref(window).len == deref(entry(window)).len + 1_u64;
  ensures deref(window).cap == deref(entry(window)).cap;
  ensures deref(window).head >= 0_u64;
  ensures deref(window).head <= deref(window).cap;
};
"#,
    ),
    (
        "prelude/take_front.wf",
        PreludeSource::Function,
        r#"fn take_front<W, T>(window: &W) -> value: T writes(window) contract {
  requires deref(window).len > 0_u64;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
  ensures deref(window).cap == deref(entry(window)).cap;
  ensures deref(window).head >= 0_u64;
  ensures deref(window).head <= deref(window).cap;
};
"#,
    ),
    (
        "prelude/swap.wf",
        PreludeSource::Function,
        r#"fn swap<T>(first: &T, second: &T) -> result: unit writes(first), writes(second);
"#,
    ),
    (
        "prelude/free_empty.wf",
        PreludeSource::Function,
        r#"fn free_empty<W>(window: W) -> result: unit pure contract {
  requires window.len == 0_u64;
};
"#,
    ),
];

#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]
    use crate::{
        ACTIVE_KERNEL_SPEC_HASH, CanonicalOutcome, CompilerLimits, FinalizeOutcome, LexOutcome,
        ParseOutcome, ResolutionOutcome, SemanticOutcome, SourceBundle, SourceInput,
        TerminalOutcome, audit_canonical, check_semantics, classify_terminals, finalize, lex,
        parse, resolve,
    };

    #[test]
    fn declarations_are_parsed_resolved_and_checked_as_ordinary_signatures() {
        let limits = CompilerLimits::default();
        let bundle = SourceBundle::with_prelude(
            &[SourceInput::new(
                "ordinary.wf",
                b"fn transfer(value: Bool) -> result: Bool pure {\n  return value;\n}\n",
            )],
            limits.source,
        )
        .expect("ordinary prelude source bundle");
        let LexOutcome::Complete(lexed) = lex(&bundle, limits.lexer) else {
            panic!("ordinary prelude lexing");
        };
        let TerminalOutcome::Complete(classified) =
            classify_terminals(&lexed, ACTIVE_KERNEL_SPEC_HASH, limits.terminals)
        else {
            panic!("ordinary prelude terminals");
        };
        let parsed = parse(classified, limits.parser);
        let ParseOutcome::Complete(parsed) = parsed else {
            panic!("ordinary prelude grammar: {parsed:?}");
        };
        let FinalizeOutcome::Complete(finalized) = finalize(parsed, limits.finalizer) else {
            panic!("ordinary prelude topology");
        };
        let canonical = audit_canonical(*finalized, limits.canonical);
        let CanonicalOutcome::Complete(canonical) = canonical else {
            panic!("ordinary prelude canonical bytes: {canonical:?}");
        };
        let resolved = resolve(canonical);
        let ResolutionOutcome::Complete(resolved) = resolved else {
            panic!("ordinary prelude resolution: {resolved:?}");
        };
        let checked = check_semantics(&resolved);
        let SemanticOutcome::Complete(checked) = checked else {
            panic!("ordinary declaration and owned transfer: {checked:?}");
        };
        let signatures = checked
            .data
            .functions
            .iter()
            .filter(|function| function.body.is_none())
            .count();
        // [PRE-1] keeps no host record: the host signatures are the standard
        // library's [PRE-2], which this unit names none of. The twenty
        // compiler-owned rows — the nine construction functions [OP-13], the
        // nine window operations [OP-10], `swap` [OP-11] and `free_empty`
        // [OP-14] — are every one of them generic, so [FN-2] gives them a
        // checked function only per concrete instance and this unit, which
        // calls none of them, has no instance of any.
        assert_eq!(signatures, 0);
        for row in crate::lowering::COMPILER_OWNED_PRELUDE_ROWS {
            assert!(
                !checked
                    .data
                    .functions
                    .iter()
                    .any(|function| function.name == row),
                "{row} is generic and this unit instantiates it nowhere"
            );
        }
        assert!(
            !checked
                .data
                .functions
                .iter()
                .any(|function| function.name == "main")
        );
        let transferred = checked
            .data
            .functions
            .iter()
            .find(|function| function.name == "transfer")
            .expect("ordinary source function");
        assert!(transferred.body.is_some());
    }

    /// [PRE-1] the records are the specification's text: the opaque struct
    /// fence and the function fence, record for record and in order. The
    /// fences write the capacity parameter as `const n: u64`, the spelling a
    /// record parses with.
    #[test]
    fn the_records_are_the_specification_text() {
        let section = crate::spec::ACTIVE_KERNEL_SPEC_TEXT
            .split("\n[PRE-1] ")
            .nth(1)
            .and_then(|rest| rest.split("\n[PRE-2] ").next())
            .expect("the specification states PRE-1");
        let fences: Vec<&str> = section.split("```\n").skip(1).step_by(2).collect();
        let [structs, _enums, functions] = fences.as_slice() else {
            panic!("PRE-1 states its structs, enums and functions in three fences");
        };
        let mut stated: Vec<String> = structs
            .split("\n\n")
            .map(|record| format!("{}\n", record.trim_end()))
            .collect();
        let mut function = String::new();
        for line in functions.lines() {
            if line.starts_with("fn ") && !function.is_empty() {
                stated.push(std::mem::take(&mut function));
            }
            function.push_str(line);
            function.push('\n');
        }
        stated.push(function);
        let carried: Vec<&str> = super::DECLARATIONS
            .iter()
            .map(|(_, _, source)| *source)
            .collect();
        assert_eq!(carried, stated);
    }
}
