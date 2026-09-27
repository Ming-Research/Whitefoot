//! The callable result ABI (compiler/src/backend/abi.rs). A stored aggregate
//! whose scalar leaves fit the return registers returns as its first-class
//! value, and a larger one returns through the caller's destination. A
//! register-returned definition is a public entry over its internal
//! destination-form body.
//!
//! The classification is checked against the leaves of the LLVM types the
//! module actually emits, counted here from the module text, so a
//! representation change that the classifier misses fails these tests
//! instead of silently sending a result through a hidden pointer or, for a
//! third floating leaf on x86-64, through the x87 stack.

use super::system::with_ir;
use super::{compile, compile_and_run, emitted_body, emitted_function};
use crate::backend::abi::{FunctionAbi, ResultAbi};
use crate::backend::emitter::llvm_type;
use crate::{IrProgram, ORDINARY_VALUES_LLVM};

/// Both sides of the x86-64 budget of three integer-class words and two
/// floating leaves, which every admitted target meets. `Four` is 16 bytes
/// and still exceeds it; `Mixed` is 40 bytes and fits it.
const BUDGET_SIDES: &[u8] = br#"struct Three {
  a: u32;
  b: u32;
  c: u32;
}

struct Four {
  a: u32;
  b: u32;
  c: u32;
  d: u32;
}

struct Floats {
  x: f64;
  y: f64;
}

struct MoreFloats {
  x: f64;
  y: f64;
  z: f64;
}

struct Mixed {
  a: u64;
  b: u64;
  c: u64;
  x: f64;
  y: f64;
}

fn three(seed: u32) -> result: Three pure {
  let b = seed +wrap 1_u32;
  let c = seed +wrap 2_u32;
  return Three(a: seed, b: b, c: c);
}

fn four(seed: u32) -> result: Four pure {
  let b = seed +wrap 1_u32;
  let c = seed +wrap 2_u32;
  let d = seed +wrap 3_u32;
  return Four(a: seed, b: b, c: c, d: d);
}

fn floats(x: f64) -> result: Floats pure {
  let y = fadd.strict(x, 1.0_f64);
  return Floats(x: x, y: y);
}

fn more_floats(x: f64) -> result: MoreFloats pure {
  let y = fadd.strict(x, 1.0_f64);
  let z = fadd.strict(x, 2.0_f64);
  return MoreFloats(x: x, y: y, z: z);
}

fn mixed(seed: u64, x: f64) -> result: Mixed pure {
  let b = seed +wrap 1_u64;
  let c = seed +wrap 2_u64;
  let y = fadd.strict(x, 1.0_f64);
  return Mixed(a: seed, b: b, c: c, x: x, y: y);
}

fn found(key: u64) -> result: Option<u64> pure {
  if key == 0_u64 {
    return None<u64>();
  }
  return Some<u64>(value: key);
}

fn sum(a: u32, b: u32) -> result: Result<u32, Overflow> pure {
  return a +checked b;
}

fn bytes(value: u8) -> result: Array<u8, 16> pure {
  return array_filled::<u8, 16>(value: value);
}

fn main() -> status: std::process::ExitStatus pure {
  let small = three(seed: 5_u32);
  if small.a != 5_u32 {
    return std::process::exit_status(code: 1_u8);
  }
  if small.c != 7_u32 {
    return std::process::exit_status(code: 2_u8);
  }
  let large = four(seed: 9_u32);
  if large.a != 9_u32 {
    return std::process::exit_status(code: 3_u8);
  }
  if large.d != 12_u32 {
    return std::process::exit_status(code: 4_u8);
  }
  let pair = floats(x: 1.5_f64);
  if fne(pair.x, 1.5_f64) {
    return std::process::exit_status(code: 5_u8);
  }
  if fne(pair.y, 2.5_f64) {
    return std::process::exit_status(code: 6_u8);
  }
  let triple = more_floats(x: 4.0_f64);
  if fne(triple.z, 6.0_f64) {
    return std::process::exit_status(code: 7_u8);
  }
  let blend = mixed(seed: 40_u64, x: 0.25_f64);
  if blend.a != 40_u64 {
    return std::process::exit_status(code: 8_u8);
  }
  if blend.c != 42_u64 {
    return std::process::exit_status(code: 9_u8);
  }
  if fne(blend.x, 0.25_f64) {
    return std::process::exit_status(code: 10_u8);
  }
  if fne(blend.y, 1.25_f64) {
    return std::process::exit_status(code: 11_u8);
  }
  match found(key: 17_u64) {
    Some(value: present) => {
      if present != 17_u64 {
        return std::process::exit_status(code: 12_u8);
      }
    }
    None() => {
      return std::process::exit_status(code: 13_u8);
    }
  }
  match found(key: 0_u64) {
    Some(value: unexpected) => {
      return std::process::exit_status(code: 14_u8);
    }
    None() => {
    }
  }
  match sum(a: 4000000000_u32, b: 300000000_u32) {
    Ok(value: wrapped) => {
      return std::process::exit_status(code: 15_u8);
    }
    Err(error: overflow) => {
    }
  }
  match sum(a: 7_u32, b: 8_u32) {
    Ok(value: total) => {
      if total != 15_u32 {
        return std::process::exit_status(code: 16_u8);
      }
    }
    Err(error: overflow) => {
      return std::process::exit_status(code: 17_u8);
    }
  }
  let filled = bytes(value: 3_u8);
  if filled[15_u64] != 3_u8 {
    return std::process::exit_status(code: 18_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

#[test]
fn stored_results_return_in_registers_exactly_when_their_leaves_fit() {
    let expected = [
        ("three", true),
        ("four", false),
        ("floats", true),
        ("more_floats", false),
        ("mixed", true),
        ("found", true),
        ("sum", true),
        ("bytes", false),
        ("main", false),
    ];
    with_ir(BUDGET_SIDES, |program| {
        let module = crate::emit_llvm(program)
            .expect("both result forms emit")
            .into_string();
        assert_classification_matches_emitted_leaves(program, &module);
        let main = emitted_function(&module, "main");
        for (name, registers) in expected {
            let function = program
                .functions()
                .iter()
                .find(|function| function.name() == name)
                .expect("fixture function");
            let result = FunctionAbi::build(program, function)
                .expect("callable ABI")
                .result();
            let entry = emitted_function(&module, name);
            let body_symbol = format!("@wf_{name}.body(");
            if !registers {
                assert!(matches!(result, ResultAbi::Destination(_)), "{name}");
                assert!(
                    entry.starts_with(&format!("define void @wf_{name}(ptr %wf.result")),
                    "{entry}"
                );
                assert!(!module.contains(&body_symbol), "{name}");
                continue;
            }
            assert!(matches!(result, ResultAbi::StoredValue(_)), "{name}");
            let ty = llvm_type(program, result.ty()).expect("result type");
            assert!(
                entry.starts_with(&format!("define {ty} @wf_{name}(")),
                "{entry}"
            );
            let header = entry.lines().next().expect("definition header");
            assert!(!header.contains("%wf.result"), "{header}");
            // The entry gives its body one slot, calls it, and returns the
            // value the body constructed there.
            assert!(entry.contains("  %wf.result = "), "{entry}");
            assert!(
                entry.contains(&format!("  call void {body_symbol}ptr %wf.result")),
                "{entry}"
            );
            assert!(
                entry.ends_with(&format!(
                    "\n  %wf.returned = load {ty}, ptr %wf.result\n  ret {ty} %wf.returned\n}}\n"
                )),
                "{entry}"
            );
            assert_eq!(entry.matches("\n  ret ").count(), 1, "{entry}");
            // The body is the destination-form definition, internal and
            // called only by its entry, so the host simplifies it on its
            // own before it inlines it there. Nothing marks it always-inline.
            let body = emitted_body(&module, name);
            assert!(
                body.starts_with(&format!(
                    "define internal void @wf_{name}.body(ptr %wf.result"
                )),
                "{body}"
            );
            assert!(
                body.lines()
                    .filter(|line| line.starts_with("  ret "))
                    .all(|line| line == "  ret void"),
                "{body}"
            );
            assert_eq!(module.matches(&body_symbol).count(), 2, "{name}");
            assert!(
                main.contains(&format!(" = call {ty} @wf_{name}(")),
                "{name}: {main}"
            );
        }
        assert!(!module.contains("alwaysinline"), "{module}");
    });
}

/// The values themselves cross call boundaries that host optimization may
/// not remove, on both sides of the budget and for both register classes.
#[test]
fn results_on_both_sides_of_the_budget_cross_retained_calls() {
    let module = super::owned_places::retain_calls(&compile(BUDGET_SIDES));
    let output = compile_and_run(&module);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

/// Signaling NaNs with distinct payloads, each checked bit for bit after it
/// crosses a call. `Pair` returns its two floating leaves in registers from
/// its entry; `Triple` keeps its destination. On x86-64 a third floating leaf
/// would return through the x87 stack, whose load quiets a signaling NaN.
const NAN_PAYLOADS: &[u8] = br#"struct Pair {
  x: f64;
  y: f64;
}

struct Triple {
  x: f64;
  y: f64;
  z: f64;
}

fn pair(x_bits: u64, y_bits: u64) -> result: Pair pure {
  let x = reinterpret::<u64, f64>(x_bits);
  let y = reinterpret::<u64, f64>(y_bits);
  return Pair(x: x, y: y);
}

fn triple(x_bits: u64, y_bits: u64, z_bits: u64) -> result: Triple pure {
  let x = reinterpret::<u64, f64>(x_bits);
  let y = reinterpret::<u64, f64>(y_bits);
  let z = reinterpret::<u64, f64>(z_bits);
  return Triple(x: x, y: y, z: z);
}

fn main() -> status: std::process::ExitStatus pure {
  let two = pair(x_bits: 9218868437227405313_u64, y_bits: 18442240474082181122_u64);
  let two_x = reinterpret::<f64, u64>(two.x);
  if two_x != 9218868437227405313_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  let two_y = reinterpret::<f64, u64>(two.y);
  if two_y != 18442240474082181122_u64 {
    return std::process::exit_status(code: 2_u8);
  }
  let three = triple(x_bits: 9218868437227405315_u64, y_bits: 18442240474082181124_u64, z_bits: 9218868437227405317_u64);
  let three_x = reinterpret::<f64, u64>(three.x);
  if three_x != 9218868437227405315_u64 {
    return std::process::exit_status(code: 3_u8);
  }
  let three_y = reinterpret::<f64, u64>(three.y);
  if three_y != 18442240474082181124_u64 {
    return std::process::exit_status(code: 4_u8);
  }
  let three_z = reinterpret::<f64, u64>(three.z);
  if three_z != 9218868437227405317_u64 {
    return std::process::exit_status(code: 5_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
"#;

/// A signaling NaN keeps every payload bit, `0x7ff0000000000001` and the
/// others above, through a register-returned result's entry and body and
/// through a destination, across calls host optimization may not remove.
/// The exit code names the first leaf whose bits changed. Admitting a third
/// floating leaf to the register budget would send `Triple` through the x87
/// stack on x86-64 and quiet its `z`.
#[test]
fn signaling_nan_payloads_cross_both_result_forms_unchanged() {
    let module = compile(NAN_PAYLOADS);
    let output = compile_and_run(&super::owned_places::retain_calls(&module));
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    // The payloads crossed both forms: `Pair`'s entry returns the value its
    // body constructed, and `Triple` returns through its destination.
    let pair = emitted_function(&module, "pair");
    assert!(pair.starts_with("define %wf.t."), "{pair}");
    assert!(
        pair.contains("  call void @wf_pair.body(ptr %wf.result, "),
        "{pair}"
    );
    let triple = emitted_function(&module, "triple");
    assert!(
        triple.starts_with("define void @wf_triple(ptr %wf.result, "),
        "{triple}"
    );
}

/// A linked definition shares its declaration's callable ABI, so every
/// linked implementation of a result returned in registers must return the
/// same first-class value, one register per leaf, whatever its C body writes.
/// The program names `std::process`, whose interface reaches `std::io`,
/// `std::text` and `std::fs`, and aliases a `std::net` type, so every host
/// module's declarations are checked.
#[test]
fn linked_definitions_return_their_declared_register_results() {
    with_ir(
        b"alias SocketAddress = std::net::SocketAddress;\n\nfn main() -> status: std::process::ExitStatus pure {\n  return std::process::exit_status(code: 0_u8);\n}\n",
        |program| {
            let module = crate::emit_llvm(program)
                .expect("prelude declarations emit")
                .into_string();
            assert_classification_matches_emitted_leaves(program, &module);
            let mut linked = Vec::new();
            for function in program.functions() {
                let result = FunctionAbi::build(program, function)
                    .expect("callable ABI")
                    .result();
                if !function.blocks().is_empty() || !matches!(result, ResultAbi::StoredValue(_)) {
                    continue;
                }
                let declared = llvm_type(program, result.ty()).expect("result type");
                let symbol = format!(" @wf_{}(", function.name());
                let definition = ORDINARY_VALUES_LLVM
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("define ")
                            .and_then(|header| header.split_once(&symbol))
                            .map(|(ty, _)| ty)
                    })
                    .unwrap_or_else(|| {
                        panic!(
                            "{} returns in registers but has no LLVM definition",
                            function.name()
                        )
                    });
                assert_eq!(
                    expanded(&module, &declared),
                    expanded(ORDINARY_VALUES_LLVM, definition),
                    "{}",
                    function.name()
                );
                linked.push(function.name().to_owned());
            }
            // `Result<u64, Utf8Error>` is the one linked result that fits,
            // named by its standard library module [MOD-10].
            assert_eq!(linked, ["std.text.host_utf8_len"]);
        },
    );
}

/// Every stored result is by value exactly when the leaves of the type the
/// module emits for it fit three integer-class words and two floating
/// leaves. The count is taken from the text, not from the classifier.
fn assert_classification_matches_emitted_leaves(program: &IrProgram, module: &str) {
    for function in program.functions() {
        let result = FunctionAbi::build(program, function)
            .expect("callable ABI")
            .result();
        if matches!(result, ResultAbi::Value(_)) {
            continue;
        }
        let ty = llvm_type(program, result.ty()).expect("result type");
        let (integer_words, floating) = leaves(&expanded(module, &ty));
        assert_eq!(
            matches!(result, ResultAbi::StoredValue(_)),
            integer_words <= 3 && floating <= 2,
            "{}: {ty} has {integer_words} integer words and {floating} floating leaves",
            function.name()
        );
    }
}

/// One LLVM type with every named type replaced by its body.
fn expanded(module: &str, ty: &str) -> String {
    let ty = ty.trim();
    if let Some(body) = module.lines().find_map(|line| {
        line.strip_prefix(ty)
            .and_then(|rest| rest.strip_prefix(" = type "))
    }) {
        return expanded(module, body);
    }
    if let Some(inner) = ty.strip_prefix('{').and_then(|rest| rest.strip_suffix('}')) {
        let fields: Vec<_> = top_level_fields(inner)
            .into_iter()
            .map(|field| expanded(module, field))
            .collect();
        return if fields.is_empty() {
            "{}".to_owned()
        } else {
            format!("{{ {} }}", fields.join(", "))
        };
    }
    if let Some(inner) = ty.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
        let (count, element) = inner.split_once(" x ").expect("array type");
        return format!("[{count} x {}]", expanded(module, element));
    }
    ty.to_owned()
}

/// Integer-class words and floating leaves of one expanded type, as LLVM's
/// return lowering assigns them: one register per scalar, two for `i128`.
fn leaves(ty: &str) -> (u64, u64) {
    let ty = ty.trim();
    if let Some(inner) = ty.strip_prefix('{').and_then(|rest| rest.strip_suffix('}')) {
        return top_level_fields(inner)
            .into_iter()
            .map(leaves)
            .fold((0, 0), |(words, floats), (more_words, more_floats)| {
                (words + more_words, floats + more_floats)
            });
    }
    if let Some(inner) = ty.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
        let (count, element) = inner.split_once(" x ").expect("array type");
        let count: u64 = count.parse().expect("array length");
        let (words, floats) = leaves(element);
        return (words * count, floats * count);
    }
    match ty {
        "i1" | "i8" | "i16" | "i32" | "i64" | "ptr" => (1, 0),
        "i128" => (2, 0),
        "float" | "double" => (0, 1),
        other => panic!("unexpected emitted scalar {other}"),
    }
}

fn top_level_fields(inner: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut depth = 0_u32;
    let mut start = 0;
    for (index, character) in inner.char_indices() {
        match character {
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                fields.push(inner[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        fields.push(last);
    }
    fields
}
