//! Union-laid-out payload enums (compiler/payload-enum-layout): their layout
//! against the target computation and LLVM's own, the memory-only emission
//! invariant, and execution of construction, matching, moves, copies,
//! references into payloads, calls and release.

use std::collections::BTreeMap;

use super::system::with_ir;
use super::{OverlapLowering, compile, compile_link_and_run, emit_lowered};
use crate::target::{
    TargetLayout, TargetStorageType, is_memory_only, is_union_enum, validate_static_storage,
};
use crate::{IrNominalKind, IrProgram, IrType};

/// The investigation's probe types: Snowghost's CSS `Component` exactly as
/// its tokenizer declares it, the Slab's handle and a 256-byte record, the
/// standard library's `IoError` results, and small enums whose product
/// returns in registers.
const LAYOUT_SOURCE: &[u8] = br#"struct Span {
  start: u32;
  end: u32;
}

enum NumberKind {
  Integer();
  Number();
}

struct Numeric {
  value: f64;
  kind: NumberKind;
  representation: Span;
}

enum HashKind {
  Id();
  Unrestricted();
}

enum ParseError {
  BadString();
  BadUrl();
  EofInString();
  EofInUrl();
  UnmatchedParenthesis();
  UnmatchedBracket();
  UnmatchedBrace();
}

enum Component {
  Ident(name: Span);
  Function(name: Span, end: u32);
  AtKeyword(name: Span);
  Hash(name: Span, kind: HashKind);
  QuotedString(value: Span);
  Url(value: Span);
  Delim(code_point: u32);
  Number(number: Numeric);
  Percentage(number: Numeric);
  Dimension(number: Numeric, unit_name: Span);
  UnicodeRange(start: u32, end: u32);
  IncludeMatch();
  DashMatch();
  PrefixMatch();
  SuffixMatch();
  SubstringMatch();
  Column();
  Whitespace();
  Cdo();
  Cdc();
  Colon();
  Semicolon();
  Comma();
  ParenthesisBlock(end: u32);
  BracketBlock(end: u32);
  BraceBlock(end: u32);
  Error(error: ParseError);
}

nocopy struct Record {
  words: Array<u64, 32>;
}

struct SlabHandle {
  index: u64;
  generation: u64;
}

fn component(code: u32) -> result: Component pure {
  return Component::Delim(code_point: code);
}

fn inserted(handle: SlabHandle) -> result: Result<SlabHandle, Record> pure {
  return Ok<SlabHandle, Record>(value: handle);
}

fn removed() -> result: Option<Record> pure {
  return None<Record>();
}

fn written(value: u64) -> result: Result<u64, std::io::IoError> pure {
  return Ok<u64, std::io::IoError>(value: value);
}

fn small(value: u64) -> result: Option<u64> pure {
  return Some<u64>(value: value);
}

fn pair(value: u64) -> result: Result<u64, u64> pure {
  return Err<u64, u64>(error: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let delim = component(code: 7_u32);
  let handle = SlabHandle(index: 1_u64, generation: 2_u64);
  let first = inserted(handle: handle);
  let second = removed();
  let third = written(value: 3_u64);
  let fourth = small(value: 4_u64);
  let fifth = pair(value: 5_u64);
  return std::process::exit_status(code: 0_u8);
}
"#;

fn host() -> TargetLayout {
    TargetLayout::host().expect("supported test target")
}

/// The selected target's size and alignment of `ty`.
fn selected(program: &IrProgram, ty: IrType) -> (u64, u64) {
    let layout = validate_static_storage(host(), program, &TargetStorageType::source(ty))
        .expect("qualified layout");
    (layout.size(), layout.align())
}

/// The ordinary sequence rule: each field at its alignment, the whole
/// rounded to the largest.
fn sequence(fields: impl IntoIterator<Item = (u64, u64)>) -> (u64, u64) {
    let (mut size, mut align) = (0_u64, 1_u64);
    for (field_size, field_align) in fields {
        size = size.next_multiple_of(field_align) + field_size;
        align = align.max(field_align);
    }
    (size.next_multiple_of(align), align)
}

const TAG: (u64, u64) = (4, 4);

/// Every enum with a payload in `program`: its link name, its selected
/// layout, its product layout (the tag followed by every variant's fields,
/// with nested enums in their own selected layouts), and each payload
/// variant's view (the tag followed by that variant's fields).
struct EnumLayout {
    link: String,
    union: bool,
    selected: (u64, u64),
    product: (u64, u64),
    views: Vec<(u32, (u64, u64))>,
}

fn enum_layouts(program: &IrProgram) -> Vec<EnumLayout> {
    let mut layouts = Vec::new();
    for nominal in program.nominals() {
        let IrNominalKind::Enum { variants } = nominal.kind() else {
            continue;
        };
        if nominal.is_tag_only_enum() {
            continue;
        }
        let field = |ty: IrType| selected(program, ty);
        let product = sequence(
            std::iter::once(TAG).chain(
                variants
                    .iter()
                    .flat_map(|variant| variant.fields())
                    .map(|declared| field(declared.ty())),
            ),
        );
        let views = variants
            .iter()
            .filter(|variant| !variant.fields().is_empty())
            .map(|variant| {
                let view = sequence(
                    std::iter::once(TAG)
                        .chain(variant.fields().iter().map(|declared| field(declared.ty()))),
                );
                (variant.tag(), view)
            })
            .collect();
        layouts.push(EnumLayout {
            link: nominal.link_name().to_owned(),
            union: is_union_enum(program.nominals(), program.elements(), nominal.id())
                .expect("eligibility"),
            selected: selected(program, IrType::Nominal(nominal.id())),
            product,
            views,
        });
    }
    layouts
}

/// The layout of the named function's result type.
fn result_layout(program: &IrProgram, function: &str) -> ((u64, u64), bool) {
    let ty = program
        .functions()
        .iter()
        .find(|candidate| candidate.name() == function)
        .unwrap_or_else(|| panic!("missing function {function}"))
        .result();
    let IrType::Nominal(id) = ty else {
        panic!("{function} returns a nominal");
    };
    (
        selected(program, ty),
        is_union_enum(program.nominals(), program.elements(), id).expect("eligibility"),
    )
}

/// The union layout of the investigation's probe types, and LLVM's own size
/// and alignment of every emitted enum value and view type, which must equal
/// target layout's: the emitter prints a size target layout computed, and
/// this checks that the printed types really have it.
#[test]
fn union_layouts_match_the_target_computation_and_the_emitted_types() {
    let layouts = with_ir(LAYOUT_SOURCE, |program| {
        // The investigation's table: union size, alignment and eligibility.
        let component = program
            .functions()
            .iter()
            .find(|function| function.name() == "component")
            .expect("component constructor")
            .result();
        assert_eq!(selected(program, component), (40, 8), "Component");
        assert_eq!(result_layout(program, "written"), ((16, 8), true));
        assert_eq!(result_layout(program, "inserted"), ((264, 8), true));
        // One payload variant: the product already is that variant's view.
        assert_eq!(result_layout(program, "removed"), ((264, 8), false));
        // Register-returned products keep their layout.
        assert_eq!(result_layout(program, "small"), ((16, 8), false));
        assert_eq!(result_layout(program, "pair"), ((24, 8), false));
        let written = program
            .functions()
            .iter()
            .find(|function| function.name() == "written")
            .expect("written")
            .result();
        let IrType::Nominal(result) = written else {
            panic!("a Result is nominal");
        };
        let IrNominalKind::Enum { variants } =
            program.nominal(result).expect("Result instance").kind()
        else {
            panic!("a Result is an enum");
        };
        let io_error = variants[1].fields()[0].ty();
        assert_eq!(selected(program, io_error), (12, 4), "IoError");

        let layouts = enum_layouts(program);
        for layout in &layouts {
            // Never above the product, which target qualification already
            // accepted against the OP-9 ceilings, and the same alignment.
            assert!(layout.selected.0 <= layout.product.0, "{}", layout.link);
            assert_eq!(layout.selected.1, layout.product.1, "{}", layout.link);
            if layout.union {
                let size = layout.views.iter().map(|(_, view)| view.0).max();
                let align = layout.views.iter().map(|(_, view)| view.1).max();
                let expected = (
                    size.expect("payload view")
                        .next_multiple_of(layout.selected.1),
                    align.expect("payload view").max(4),
                );
                assert_eq!(layout.selected, expected, "{}", layout.link);
            } else {
                assert_eq!(layout.selected, layout.product, "{}", layout.link);
            }
        }
        layouts
    });
    assert!(layouts.iter().any(|layout| layout.union));

    // LLVM's size and alignment of every emitted enum and view type.
    let llvm = compile(LAYOUT_SOURCE);
    let mut probes = BTreeMap::new();
    for layout in &layouts {
        probes.insert(format!("wf.t.{}", layout.link), layout.selected);
        for (tag, view) in &layout.views {
            let name = format!("wf.t.{}.v{tag}", layout.link);
            let declared = llvm.contains(&format!("%{name} = type"));
            assert_eq!(declared, layout.union, "{name}");
            if layout.union {
                probes.insert(name, *view);
            }
        }
        let declaration = llvm
            .lines()
            .find_map(|line| line.strip_prefix(&format!("%wf.t.{} = type ", layout.link)))
            .unwrap_or_else(|| panic!("declared enum {}", layout.link));
        if layout.union {
            assert_eq!(
                declaration.split(", ").nth(1),
                Some(format!("[{} x i8]", layout.selected.0 - 4).as_str()),
                "{declaration}"
            );
        } else {
            assert!(!declaration.contains(" x i8], [0 x "), "{declaration}");
        }
    }
    let entries: Vec<_> = probes
        .keys()
        .flat_map(|name| {
            [
                format!("i64 ptrtoint (ptr getelementptr (%{name}, ptr null, i32 1) to i64)"),
                format!(
                    "i64 ptrtoint (ptr getelementptr ({{ i1, %{name} }}, ptr null, i32 0, i32 1) to i64)"
                ),
            ]
        })
        .collect();
    let module = format!(
        "{llvm}\n@wf_test_layouts = constant [{} x i64] [{}]\n",
        entries.len(),
        entries.join(", ")
    );
    let observer = format!(
        r#"#include <inttypes.h>
#include <stdio.h>

extern const uint64_t wf_test_layouts[{}];

__attribute__((constructor)) static void wf_test_print_layouts(void) {{
    for (unsigned index = 0; index < {}; ++index)
        printf("%" PRIu64 "\n", wf_test_layouts[index]);
}}
"#,
        entries.len(),
        entries.len()
    );
    let output = compile_link_and_run(&module, Some(&observer), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let measured: Vec<u64> = String::from_utf8(output.stdout)
        .expect("decimal layouts")
        .lines()
        .map(|line| line.parse().expect("one decimal per line"))
        .collect();
    let expected: Vec<u64> = probes
        .values()
        .flat_map(|(size, align)| [*size, *align])
        .collect();
    assert_eq!(
        measured,
        expected,
        "{:?}",
        probes.keys().collect::<Vec<_>>()
    );
}

/// The program every execution case below runs: union-laid-out enums
/// (`Component`, `Holder`, `Outer`) constructed in every variant, matched by
/// value and through references, copied, moved through calls, results,
/// block edges and a loop that exchanges two of them, exchanged by `swap`,
/// overwritten with another variant whole and as a struct field, written
/// through references into their payloads, and held in a struct, an
/// `Option`, a `Box`, a boxed and an inline `Slots`, a `Ring` whose live
/// elements wrap around its end and an `Array`, with `Box` and `Slots` owners
/// in several variants. It exits 0 when every observation matches.
const PROGRAM: &[u8] = include_bytes!("payload_enums.wf");

/// Counts allocations, refuses a release of anything not allocated, and
/// reports the allocations still live when the program ends. The overlap
/// lowering runs handed-out calls on the parallel runtime's worker threads,
/// and contexts may run on several drivers, so every access to the ledger
/// holds its lock.
const ALLOCATION_OBSERVER: &str = r#"#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>

#define WF_TEST_HELD 256
static void *held[WF_TEST_HELD];
static unsigned count;
static atomic_flag ledger = ATOMIC_FLAG_INIT;

static void lock_ledger(void) {
    while (atomic_flag_test_and_set_explicit(&ledger, memory_order_acquire)) {}
}

static void unlock_ledger(void) {
    atomic_flag_clear_explicit(&ledger, memory_order_release);
}

void *wf_test_allocate(size_t size) {
    void *allocation = malloc(size);
    lock_ledger();
    if (allocation == NULL || count == WF_TEST_HELD) abort();
    held[count++] = allocation;
    unlock_ledger();
    return allocation;
}

void wf_test_release(void *allocation) {
    lock_ledger();
    for (unsigned index = 0; index < count; ++index) {
        if (held[index] == allocation) {
            held[index] = NULL;
            unlock_ledger();
            free(allocation);
            return;
        }
    }
    abort();
}

__attribute__((destructor)) static void wf_test_report(void) {
    unsigned live = 0;
    lock_ledger();
    for (unsigned index = 0; index < count; ++index) live += held[index] != NULL;
    printf("allocated=%u live=%u\n", count, live);
    unlock_ledger();
}
"#;

fn observed(module: &str) -> String {
    super::owned_places::retain_calls(module)
        .replace("@malloc(", "@wf_test_allocate(")
        .replace("@free(", "@wf_test_release(")
}

/// Every construction, match, move, copy, call and release of the program
/// computes what the source says, under both lowerings and with every call
/// boundary kept, and every owner in every variant is released exactly once.
#[test]
fn union_enums_construct_match_move_copy_and_release_their_owners() {
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module = emit_lowered(PROGRAM, overlap);
        let output = compile_link_and_run(&observed(&module), Some(ALLOCATION_OBSERVER), &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        let report = String::from_utf8(output.stdout).expect("report");
        assert!(report.ends_with(" live=0\n"), "{overlap:?}: {report}");
        assert!(!report.starts_with("allocated=0 "), "{overlap:?}: {report}");
    }
}

/// A reference into a union-laid-out enum's payload names that variant's
/// field: `bump` passes the `stamp` payload to a callee that writes through
/// it and writes the boxed `cell` through another, and `peek` reads both
/// back through the same kind of reference. The addresses are formed through
/// the variant's view, never through a flattened product index.
#[test]
fn a_payload_reference_addresses_its_variants_view() {
    let llvm = compile(PROGRAM);
    let holder = with_ir(PROGRAM, |program| {
        let ty = program
            .functions()
            .iter()
            .find(|function| function.name() == "relay")
            .expect("relay")
            .result();
        let IrType::Nominal(id) = ty else {
            panic!("Holder is nominal");
        };
        assert!(is_union_enum(program.nominals(), program.elements(), id).expect("eligibility"));
        program.nominal(id).expect("Holder").link_name().to_owned()
    });
    let bump = super::emitted_function(&llvm, "bump");
    let view = format!("getelementptr inbounds %wf.t.{holder}.v0, ptr ");
    for field in [", i32 0, i32 1", ", i32 0, i32 2"] {
        assert!(
            bump.lines()
                .any(|line| line.contains(&view) && line.ends_with(field)),
            "{bump}"
        );
    }
    // The value type itself is addressed only at its tag, field 0.
    let value = format!("getelementptr inbounds %wf.t.{holder}, ptr ");
    assert!(
        bump.lines()
            .filter(|line| line.contains(&value))
            .all(|line| line.ends_with(", i32 0, i32 0")),
        "{bump}"
    );
    let output = super::compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

/// The emitted type names of `source`'s memory-only nominals: its
/// union-laid-out enums and the structs and enums that hold one inline.
fn memory_only_type_names(source: &[u8]) -> Vec<String> {
    with_ir(source, |program| {
        program
            .nominals()
            .iter()
            .filter(|nominal| {
                is_memory_only(
                    program.nominals(),
                    program.elements(),
                    IrType::Nominal(nominal.id()),
                )
                .expect("memory-only classification")
            })
            .map(|nominal| format!("%wf.t.{}", nominal.link_name()))
            .collect()
    })
}

/// Every line of `llvm` that mentions one of `names` is a type declaration,
/// an address computation, a frame reservation or the zero fill of a
/// construction.
fn assert_never_first_class(names: &[String], llvm: &str, context: &str) {
    for line in llvm.lines() {
        let mentions = names.iter().any(|name| {
            line.match_indices(name.as_str()).any(|(at, _)| {
                !line[at + name.len()..]
                    .starts_with(|next: char| next == '.' || next.is_ascii_alphanumeric())
            })
        });
        if !mentions {
            continue;
        }
        assert!(
            line.contains(" = type ")
                || line.contains("getelementptr")
                || line.contains(" = alloca ")
                || line.contains(" zeroinitializer, ptr "),
            "{context}: a memory-only value is first-class in `{line}`"
        );
    }
}

/// A memory-only value, a union-laid-out enum or an aggregate holding one
/// inline, is never an LLVM first-class value: every emitted mention of its
/// type is a type declaration, an address computation, a frame reservation or
/// the zero fill of a construction. Loads, stores, phis, arguments, returns,
/// `extractvalue` and `insertvalue` of it would each carry it through a type
/// LLVM cannot represent it in.
#[test]
fn memory_only_values_are_never_first_class() {
    let names = memory_only_type_names(PROGRAM);
    assert!(names.len() >= 5, "{names:?}");
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let llvm = emit_lowered(PROGRAM, overlap);
        assert_never_first_class(&names, &llvm, &format!("{overlap:?}"));
    }
}

/// A waiting program whose waiting calls take and return union-laid-out
/// enums (design/compiler/waiting-contexts.md): `build` returns one through
/// its destination to a direct waiting call, two bound spawns construct
/// one each in the starting frame's slot, which their awaits move into the
/// bindings [WAIT-3], a third bound start's binding is released
/// without being read, and an unbound start takes one by value into its
/// context's argument block, whose wrapper consumes it. `Piece` is a union
/// enum nested in `Holder`'s `Nested` variant.
const WAITING_PROGRAM: &[u8] = br#"struct Span {
  start: u32;
  end: u32;
}

enum Piece {
  Word(span: Span, weight: u64);
  Mark(code: u32);
  Gap();
}

enum Holder {
  Boxed(cell: Box<u64>, stamp: u64);
  Many(values: Box<Slots<u64>>);
  Nested(inner: Piece, extra: Box<u64>);
  Nothing();
}

fn build(kind: u64, seed: u64) -> result: Holder pure waits {
  let next = seed +wrap 1_u64;
  if kind == 0_u64 {
    let cell = box_new::<u64>(value: seed);
    return Holder::Boxed(cell: move cell, stamp: next);
  }
  if kind == 1_u64 {
    let values = box_slots_new::<u64>(capacity: 2_u64);
    place_back(window: &values.inner, value: seed);
    place_back(window: &values.inner, value: next);
    return Holder::Many(values: move values);
  }
  if kind == 2_u64 {
    let extra = box_new::<u64>(value: seed);
    let span = Span(start: 1_u32, end: 5_u32);
    let inner = Piece::Word(span: span, weight: next);
    return Holder::Nested(inner: inner, extra: move extra);
  }
  return Holder::Nothing();
}

fn weight(holder: Holder) -> result: u64 pure waits {
  match move holder {
    Boxed(cell: c, stamp: s) => {
      return c.inner +wrap s;
    }
    Many(values: v) => {
      let count = v.inner.len;
      return count +wrap 1000_u64;
    }
    Nested(inner: i, extra: e) => {
      match i {
        Word(span: s, weight: w) => {
          let end = cvt::<u32, u64>(s.end);
          let sum = end +wrap w;
          return sum +wrap e.inner;
        }
        Mark(code: c) => {
          let code = cvt::<u32, u64>(c);
          return code;
        }
        Gap() => {
          return 0_u64;
        }
      }
    }
    Nothing() => {
      return 7777_u64;
    }
  }
}

fn main() -> status: std::process::ExitStatus pure waits {
  let direct = build(kind: 0_u64, seed: 3_u64);
  let first = spawn build(kind: 2_u64, seed: 5_u64);
  let second = spawn build(kind: 1_u64, seed: 7_u64);
  let given = build(kind: 0_u64, seed: 9_u64);
  spawn weight(holder: move given);
  let kept = spawn build(kind: 2_u64, seed: 11_u64);
  let direct_weight = weight(holder: move direct);
  if direct_weight != 7_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let first_weight = weight(holder: move first);
  if first_weight != 16_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let second_weight = weight(holder: move second);
  if second_weight != 1002_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// Union-laid-out enums cross waiting calls and contexts as memory: every
/// result, bound or direct, and every argument handed to a context is
/// copied by memmove and never first-class, every observation matches, and
/// every owner is released exactly once. Contexts may run on several
/// drivers, which the observer's lock serves too.
#[test]
fn union_enums_cross_waiting_calls_and_contexts() {
    let names = memory_only_type_names(WAITING_PROGRAM);
    assert!(names.len() >= 2, "{names:?}");
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module = emit_lowered(WAITING_PROGRAM, overlap);
        assert!(
            module.contains("%wf.ctx.group."),
            "{overlap:?}: a bound start"
        );
        assert!(
            module.contains("@wf__context_launch(ptr %wf.ctx.group,"),
            "{overlap:?}: an unbound start"
        );
        assert_never_first_class(&names, &module, &format!("{overlap:?}"));
        let output = compile_link_and_run(&observed(&module), Some(ALLOCATION_OBSERVER), &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        let report = String::from_utf8(output.stdout).expect("report");
        assert!(report.ends_with(" live=0\n"), "{overlap:?}: {report}");
        assert!(!report.starts_with("allocated=0 "), "{overlap:?}: {report}");
    }
}

/// A shared object whose state is a union-laid-out enum [SHARE-1]: `main`
/// creates it holding a boxed run, reads it through an atomic statement, and
/// four contexts each replace it with a boxed cell, so every replaced state
/// and, with the last handle, the final state are released. The shared
/// object's release helper releases the state in place, never as a loaded
/// value.
const SHARED_PROGRAM: &[u8] = br#"enum Holder {
  Boxed(cell: Box<u64>, stamp: u64);
  Many(values: Box<Slots<u64>>);
  Nothing();
}

fn peek(holder: &Holder) -> result: u64 reads(holder) {
  match holder^ {
    Boxed(cell: c, stamp: s) => {
      return c^.inner +wrap s^;
    }
    Many(values: v) => {
      let count = v^.inner.len;
      return count +wrap 1000_u64;
    }
    Nothing() => {
      return 7777_u64;
    }
  }
}

fn fill(held: Shared<Holder>, seed: u64) -> result: unit pure waits {
  atomic state = &held {
    let cell = box_new::<u64>(value: seed);
    let next = Holder::Boxed(cell: move cell, stamp: 100_u64);
    set state^ = move next;
  }
  return unit;
}

fn fill_all(held: &Shared<Holder>) -> result: unit reads(held) waits {
  for @start (index in 0_u64..4_u64) {
    let handle = shared_share::<Holder>(shared: held);
    spawn fill(held: move handle, seed: index);
  }
  return unit;
}

fn main() -> status: std::process::ExitStatus pure waits {
  let values = box_slots_new::<u64>(capacity: 2_u64);
  place_back(window: &values.inner, value: 1_u64);
  let start = Holder::Many(values: move values);
  let held = shared_new::<Holder>(value: move start);
  let seen = 0_u64;
  atomic state = &held {
    set seen = peek(holder: state);
  }
  if seen != 1001_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  fill_all(held: &held);
  let last = 0_u64;
  atomic state = &held {
    set last = peek(holder: state);
  }
  if last < 100_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  if last > 103_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// A union-laid-out enum lives in a shared object as memory: it is never
/// first-class, the observations match, and every owner, replaced or final,
/// is released exactly once.
#[test]
fn union_enums_live_in_shared_objects() {
    let names = memory_only_type_names(SHARED_PROGRAM);
    assert!(!names.is_empty(), "{names:?}");
    for overlap in [OverlapLowering::Off, OverlapLowering::On] {
        let module = emit_lowered(SHARED_PROGRAM, overlap);
        assert_never_first_class(&names, &module, &format!("{overlap:?}"));
        let output = compile_link_and_run(&observed(&module), Some(ALLOCATION_OBSERVER), &[]);
        assert_eq!(output.status.code(), Some(0), "{overlap:?}: {output:?}");
        let report = String::from_utf8(output.stdout).expect("report");
        assert!(report.ends_with(" live=0\n"), "{overlap:?}: {report}");
        assert!(!report.starts_with("allocated=0 "), "{overlap:?}: {report}");
    }
}

/// A runtime-capacity window of union-laid-out elements requests exactly its
/// header and the union stride per slot: 40 bytes per Snowghost-shaped
/// `Component`, where the product layout requested 168, so the 8,000,000
/// components of the investigation's stylesheet request 320,000,016 bytes
/// instead of 1,344,000,016. Untouched slots are never committed.
#[test]
fn a_window_of_union_enums_requests_the_union_stride() {
    let source = String::from_utf8(LAYOUT_SOURCE.to_vec())
        .expect("source text")
        .replace(
            "  return std::process::exit_status(code: 0_u8);\n}\n",
            "  let components = box_slots_new::<Component>(capacity: 8000000_u64);\n  place_back(window: &components.inner, value: delim);\n  return std::process::exit_status(code: 0_u8);\n}\n",
        );
    let module = compile(source.as_bytes()).replace("@malloc(", "@wf_test_allocate(");
    let observer = r#"#include <stdio.h>
#include <stdlib.h>

void *wf_test_allocate(size_t size) {
    printf("%zu\n", size);
    return malloc(size);
}
"#;
    let output = compile_link_and_run(&module, Some(observer), &[]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let requests = String::from_utf8(output.stdout).expect("sizes");
    // The block's two header words, then 8,000,000 slots of 40 bytes.
    assert!(
        requests.lines().any(|line| line == "320000016"),
        "{requests}"
    );
}
