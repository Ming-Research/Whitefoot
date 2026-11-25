//! Retained ordinary computation and its complete structured dependency closure.

use super::{emit_lowered, parallel::function_body, system::with_mutated_ir_lowering};
use crate::{IrConstant, IrInstruction, IrOperation, OverlapLowering};

fn assert_metadata(
    ordinary: &crate::backend::emission::Module,
    combined: &crate::backend::emission::Module,
    references: &crate::backend::emission::References,
) {
    let mut pending = references.types.clone();
    let mut visited = std::collections::BTreeSet::new();
    while let Some(name) = pending.pop_first() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let reference = ordinary.types.get(&name).unwrap();
        assert_eq!(
            combined.types.get(&name),
            Some(reference),
            "type definition {name}"
        );
        pending.extend(reference.references.types.iter().cloned());
        for group in &reference.references.attributes {
            assert_eq!(
                combined.attributes.get(group),
                ordinary.attributes.get(group)
            );
        }
    }
    for group in &references.attributes {
        assert_eq!(
            combined.attributes.get(group),
            ordinary.attributes.get(group),
            "attribute group {group}"
        );
    }
}

/// Restore only identities allocated to the imported ordinary closure.
/// Instruction operands, calling conventions and layouts remain exact.
pub(super) fn ordinary_symbol(name: &str) -> Option<String> {
    name.strip_prefix("wf__par_seq_")
        .map(|suffix| format!("wf_{suffix}"))
        .or_else(|| name.strip_prefix("wf__par_seq.").map(str::to_owned))
        .or_else(|| name.strip_prefix(".wf__par_seq_global.").map(str::to_owned))
}

pub(super) fn restore_symbols(module: &crate::backend::emission::Module, text: &str) -> String {
    let mapping = module
        .entities
        .iter()
        .filter_map(|entity| ordinary_symbol(&entity.name).map(|name| (entity.name.as_str(), name)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let bytes = text.as_bytes();
    let mut result = String::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let symbol = bytes[at] == b'@';
        if symbol {
            at += 1;
        }
        if bytes[at] == b'"' {
            at += 1;
            let name_start = at;
            while bytes[at] != b'"' {
                at += if bytes[at] == b'\\' { 3 } else { 1 };
            }
            let name = &text[name_start..at];
            at += 1;
            if symbol && let Some(restored) = mapping.get(name) {
                result.push_str(&format!("@\"{restored}\""));
            } else {
                result.push_str(&text[start..at]);
            }
        } else if symbol {
            let name_start = at;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric() || b"-.$_".contains(&bytes[at]))
            {
                at += 1;
            }
            let name = &text[name_start..at];
            result.push('@');
            result.push_str(mapping.get(name).map_or(name, String::as_str));
        } else if bytes[at] == b';' {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            result.push_str(&text[start..at]);
        } else {
            let character = text[at..].chars().next().unwrap();
            result.push(character);
            at += character.len_utf8();
        }
    }
    result
}

#[test]
fn ordinary_symbol_restoration_keeps_literals_and_unrecorded_targets() {
    use crate::backend::emission::{FunctionBody, Module, References, Signature};
    let mut module = Module::default();
    for name in ["wf__par_seq_main", "wf__par_seq.wf.drop.cell"] {
        let mut body = FunctionBody::default();
        body.open_block("entry".to_owned());
        body.instructions("  ret void\n", &[]);
        module.define(
            Signature::new(name, "void", Vec::new())
                .define(body, "")
                .unwrap(),
        );
    }
    module.global(
        ".wf__par_seq_global.table".to_owned(),
        "constant",
        "ptr".to_owned(),
        "@wf__par_seq_main".to_owned(),
        None,
        References::default(),
    );
    let actual = "@wf__par_seq_main @\"wf__par_seq.wf.drop.cell\" @.wf__par_seq_global.table @wf__par_seq_main.extra @wf_main \"@wf__par_seq_main\" ; @wf__par_seq_main\n";
    let expected = "@wf_main @\"wf.drop.cell\" @table @wf__par_seq_main.extra @wf_main \"@wf__par_seq_main\" ; @wf__par_seq_main\n";
    assert_eq!(restore_symbols(&module, actual), expected);
    assert_ne!(
        restore_symbols(&module, "call void @wf__par_seq.wf.drop.cell()"),
        "call void @wf.drop.cell(i64 1)"
    );
}

pub(super) fn restore_definition(
    actual: &crate::backend::emission::Module,
    expected: &crate::backend::emission::Module,
    actual_entity: &crate::backend::emission::Entity,
    expected_entity: &crate::backend::emission::Entity,
    text: &str,
) -> String {
    restore_symbols(
        actual,
        &restore_metadata(actual, expected, actual_entity, expected_entity, text),
    )
}

/// Only metadata IDs vary: prove an injective graph correspondence, including
/// distinct/self edges, before substituting tokens in the instruction text.
pub(super) fn restore_metadata(
    actual: &crate::backend::emission::Module,
    expected: &crate::backend::emission::Module,
    actual_entity: &crate::backend::emission::Entity,
    expected_entity: &crate::backend::emission::Entity,
    text: &str,
) -> String {
    use std::collections::{BTreeMap, BTreeSet};
    let roots = |entity: &crate::backend::emission::Entity| {
        entity
            .references
            .metadata
            .union(&entity.declaration_references.metadata)
            .copied()
            .collect::<BTreeSet<_>>()
    };
    let actual_roots = roots(actual_entity);
    let expected_roots = roots(expected_entity);
    assert_eq!(actual_roots.len(), expected_roots.len());
    let mut pending = actual_roots
        .into_iter()
        .zip(expected_roots)
        .collect::<Vec<_>>();
    let mut mapping = BTreeMap::new();
    let mut targets = BTreeSet::new();
    while let Some((from, to)) = pending.pop() {
        if let Some(previous) = mapping.insert(from, to) {
            assert_eq!(previous, to, "metadata mapping must preserve sharing");
            continue;
        }
        assert!(
            targets.insert(to),
            "distinct metadata identities must remain distinct"
        );
        let left = actual
            .metadata
            .get(&from)
            .expect("actual metadata is defined");
        let right = expected
            .metadata
            .get(&to)
            .expect("expected metadata is defined");
        assert_eq!(left.references.len(), right.references.len());
        pending.extend(
            left.references
                .iter()
                .copied()
                .zip(right.references.iter().copied()),
        );
    }
    for (from, to) in &mapping {
        assert_eq!(
            metadata_tokens(&actual.metadata[from].body, &mapping),
            expected.metadata[to].body,
            "metadata !{from} has the ordinary meaning of !{to}"
        );
    }
    metadata_tokens(text, &mapping)
}

fn metadata_tokens(text: &str, mapping: &std::collections::BTreeMap<u64, u64>) -> String {
    let mut result = String::new();
    let mut characters = text.chars().peekable();
    let mut quoted = false;
    let mut comment = false;
    while let Some(character) = characters.next() {
        if quoted && character == '\\' {
            result.push(character);
            result.push(characters.next().expect("escaped character"));
            continue;
        }
        if !comment && character == '"' {
            quoted = !quoted;
        }
        if !quoted && character == ';' {
            comment = true;
        }
        if character == '\n' {
            comment = false;
        }
        result.push(character);
        if !quoted
            && !comment
            && character == '!'
            && characters.peek().is_some_and(char::is_ascii_digit)
        {
            let mut digits = String::new();
            while characters.peek().is_some_and(char::is_ascii_digit) {
                digits.push(characters.next().unwrap());
            }
            let id = digits.parse::<u64>().unwrap();
            result.push_str(
                &mapping
                    .get(&id)
                    .expect("every metadata token has a recorded dependency")
                    .to_string(),
            );
        }
    }
    result
}

#[test]
fn metadata_identity_comparison_rejects_changed_meaning_or_sharing() {
    use crate::backend::emission::{FunctionBody, Module, Signature};
    let mut expected = Module::default();
    for id in [0, 1] {
        expected.metadata_node(id, format!("distinct !{{!{id}}}"), [id].into());
        expected.metadata_node(id + 2, format!("!{{!{id}}}"), [id].into());
    }
    let mut body = FunctionBody::default();
    body.open_block("entry".to_owned());
    body.instructions("  ret void\n", &[]);
    body.references.metadata.extend([2, 3]);
    expected.define(
        Signature::new("test", "void", Vec::new())
            .define(body, "")
            .unwrap(),
    );
    let entity = &expected.entities[0];
    assert_eq!(
        restore_metadata(&expected, &expected, entity, entity, "!2 !3"),
        "!2 !3"
    );
    for defect in ["distinctness", "collapsed identity", "missing edge"] {
        let mut actual = expected.clone();
        match defect {
            "distinctness" => actual.metadata.get_mut(&0).unwrap().body = "!{!0}".to_owned(),
            "collapsed identity" => {
                let node = actual.metadata.get_mut(&3).unwrap();
                node.body = "!{!0}".to_owned();
                node.references = [0].into();
            }
            "missing edge" => actual.metadata.get_mut(&2).unwrap().references.clear(),
            _ => unreachable!(),
        }
        assert!(
            std::panic::catch_unwind(|| restore_metadata(
                &actual,
                &expected,
                &actual.entities[0],
                entity,
                "!2 !3"
            ))
            .is_err(),
            "{defect}"
        );
    }
}

const SOURCE: &[u8] = br#"const increments: Array<u64, 2> =[7_u64, 9_u64];

fn helper(seed: u64, count: u64) -> result: u64 pure {
  let total = seed +wrap increments[0_u64];
  for @sum (i in 0_u64..count) {
    set total = total +wrap i;
  }
  return total;
}

fn paired(seed: u64, count: u64) -> result: u64 pure {
  let next_seed = seed +wrap 1_u64;
  let first = helper(seed: seed, count: count);
  let second = helper(seed: next_seed, count: count);
  return first +wrap second;
}
"#;

#[test]
fn canonical_source_closure_precedes_loop_synthesis_and_preserves_parallel_bodies() {
    let ordinary = emit_lowered(SOURCE, OverlapLowering::Off);
    with_mutated_ir_lowering(SOURCE, OverlapLowering::On, |program| {
        let canonical = program
            .canonical_sequential
            .as_deref()
            .expect("retained Off graph");
        assert!(canonical.canonical_sequential.is_none());
        assert!(program.functions().iter().any(|f| f.synthesis().is_some()));
        let mut legacy = program.clone();
        legacy.canonical_sequential = None;
        let legacy_module = crate::backend::emitter::emit_llvm(&legacy).unwrap();
        let before = crate::backend::emitter::emit_llvm(program).expect("both worlds emit");
        for entity in legacy_module
            .model
            .entities
            .iter()
            .filter(|entity| !entity.name.starts_with("wf__par_seq_"))
        {
            assert!(
                before.model.entities.contains(entity),
                "canonical import must preserve the previous parallel definition {}",
                entity.name
            );
        }
        for name in ["paired", "helper"] {
            let clone = function_body(&before, &format!("@wf__par_seq_{name}"));
            assert_eq!(
                restore_symbols(&before.model, clone),
                function_body(&ordinary, &format!("@wf_{name}")),
                "{name} must be the complete ordinary computation, not a synthesized chunk call"
            );
            assert!(!clone.contains("@wf__par_chunk_"));
            assert!(!clone.contains("@wf__par_seq__par_chunk_"));
        }
        assert!(
            function_body(&before, "@wf__par_seq_paired").contains("call i64 @wf__par_seq_helper(")
        );
        let reference = program
            .canonical_sequential
            .as_deref_mut()
            .expect("retained Off graph");
        let ordinary_module = crate::backend::emitter::emit_llvm(reference).unwrap();
        assert!(
            ordinary_module
                .model
                .entities
                .iter()
                .any(|entity| entity.global && entity.name.starts_with(".wf_const.")),
            "the fixture must expose rodata"
        );
        for constant in ordinary_module
            .model
            .entities
            .iter()
            .filter(|entity| entity.global && entity.name.starts_with(".wf_const."))
        {
            assert!(
                before.model.entities.contains(constant),
                "ordinary constants keep their values"
            );
        }
        assert_eq!(before.model.attributes, ordinary_module.model.attributes);
        let helper = reference
            .functions
            .iter_mut()
            .find(|f| f.name() == "helper")
            .unwrap();
        let changed = helper
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find_map(|instruction| match instruction {
                IrInstruction::Define {
                    operation: IrOperation::Constant(IrConstant::Integer { bits, .. }),
                    ..
                } => Some(bits),
                _ => None,
            })
            .expect("the loop has an integer constant");
        *changed = changed.wrapping_add(1);
        let after =
            crate::backend::emitter::emit_llvm(program).expect("mutated canonical graph emits");
        assert_ne!(
            function_body(&before, "@wf__par_seq_helper"),
            function_body(&after, "@wf__par_seq_helper")
        );
        for entity in before.model.entities.iter().filter(|entity| {
            !entity.name.starts_with("wf__par_seq_") && !entity.name.starts_with(".wf__par_seq")
        }) {
            assert!(
                after.model.entities.contains(entity),
                "canonical computation must not change the parallel definition {}",
                entity.name
            );
        }
    });
}

#[test]
fn canonical_aggregate_closure_keeps_public_entries_destination_bodies_and_dependencies() {
    let source = br#"struct Pair {
  left: u64;
  right: u64;
}

fn make(seed: u64) -> result: Pair pure {
  let right = seed +wrap 3_u64;
  return Pair(left: seed, right: right);
}

fn paired(seed: u64) -> result: Pair pure {
  let next_seed = seed +wrap 1_u64;
  let first = make(seed: seed);
  let second = make(seed: next_seed);
  let left = first.left +wrap second.left;
  let right = first.right +wrap second.right;
  return Pair(left: left, right: right);
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = paired(seed: 7_u64);
  if pair.left == 15_u64 {
    if pair.right == 21_u64 {
      return std::process::exit_status(code: 0_u8);
    }
  }
  return std::process::exit_status(code: 1_u8);
}
"#;
    let overlap = OverlapLowering::On;
    with_mutated_ir_lowering(source, overlap, |program| {
        let ordinary =
            crate::backend::emitter::emit_llvm(program.canonical_sequential.as_deref().unwrap())
                .unwrap();
        let combined = crate::backend::emitter::emit_llvm(program).unwrap();
        let mut pending = vec!["wf_paired".to_owned()];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            let Some(reference) = ordinary
                .model
                .entities
                .iter()
                .find(|entity| entity.name == name)
            else {
                let declaration = ordinary
                    .model
                    .declarations
                    .get(&name)
                    .expect("recorded external dependency");
                assert_eq!(
                    combined.model.declarations.get(&name),
                    Some(declaration),
                    "external binding {name}"
                );
                assert_metadata(&ordinary.model, &combined.model, &declaration.references);
                continue;
            };
            assert!(
                !reference.global && reference.linkage != crate::backend::emission::Linkage::Weak,
                "the aggregate fixture's computation closure must consist of ordinary functions"
            );
            let clone_name = format!("wf__par_seq_{}", name.strip_prefix("wf_").unwrap());
            let clone = combined
                .model
                .entities
                .iter()
                .find(|entity| entity.name == clone_name)
                .unwrap_or_else(|| panic!("missing closure function {clone_name}"));
            for (actual, expected) in [
                (&clone.header, &reference.header),
                (&clone.hidden_header, &reference.hidden_header),
                (&clone.declaration, &reference.declaration),
                (&clone.body, &reference.body),
            ] {
                assert_eq!(
                    restore_definition(&combined.model, &ordinary.model, clone, reference, actual),
                    *expected,
                    "complete canonical entity {name}"
                );
            }
            assert_eq!(clone.references.types, reference.references.types);
            assert_eq!(clone.references.attributes, reference.references.attributes);
            assert_metadata(&ordinary.model, &combined.model, &reference.references);
            assert_metadata(
                &ordinary.model,
                &combined.model,
                &reference.declaration_references,
            );
            assert_eq!(
                clone.declaration_references,
                reference.declaration_references
            );
            assert_eq!(
                clone
                    .references
                    .symbols
                    .iter()
                    .map(|name| ordinary_symbol(name).unwrap_or_else(|| name.clone()))
                    .collect::<std::collections::BTreeSet<_>>(),
                reference.references.symbols
            );
            pending.extend(reference.references.symbols.iter().cloned());
        }
        for required in ["wf_paired", "wf_paired.body", "wf_make", "wf_make.body"] {
            assert!(
                visited.contains(required),
                "both public and destination forms must be reached: {visited:?}"
            );
        }
    });
    let output = super::compile_and_run(&emit_lowered(source, overlap));
    assert!(
        output.status.success(),
        "the complete canonical program must return 15/21: {output:?}"
    );
}
