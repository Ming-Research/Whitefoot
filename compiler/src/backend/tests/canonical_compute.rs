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
                clone.replace("@wf__par_seq_", "@wf_"),
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
                    actual.replace("@wf__par_seq_", "@wf_"),
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
                    .map(|name| name.replacen("wf__par_seq_", "wf_", 1))
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
