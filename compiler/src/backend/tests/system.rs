//! Ordinary callable ABI and standard library behavior. C2 retires semantic-ID,
//! target-qualification, input-label and implicit-release assertions; the
//! preserved observable cases execute the same ordinary source as the corpus.

use crate::{
    ACTIVE_KERNEL_SPEC_HASH, CanonicalOutcome, FinalizeOutcome, IrProgram, LexOutcome,
    OverlapLowering, ParseOutcome, ResolutionOutcome, SemanticOutcome, SourceBundle, SourceInput,
    TerminalLimits, TerminalOutcome, audit_canonical, check_semantics, classify_terminals,
    finalize, lex, lower_checked_with_layout, parse, resolve,
};

use crate::target::TargetLayout;

use super::{
    CANONICAL_LIMITS, FINALIZE_LIMITS, LEX_LIMITS, PARSE_LIMITS, SOURCE_LIMITS, compile,
    compile_and_run, compile_and_run_with, compile_rejection, emitted_function,
};

pub(super) fn with_ir<R>(source: &[u8], run: impl FnOnce(&IrProgram) -> R) -> R {
    with_mutated_ir(source, |program| run(program))
}

pub(super) fn with_mutated_ir<R>(source: &[u8], run: impl FnOnce(&mut IrProgram) -> R) -> R {
    with_mutated_ir_lowering(source, OverlapLowering::Off, run)
}

pub(super) fn with_parallel_ir<R>(source: &[u8], run: impl FnOnce(&IrProgram) -> R) -> R {
    with_mutated_ir_lowering(source, OverlapLowering::On, |program| run(program))
}

pub(super) fn with_mutated_ir_lowering<R>(
    source: &[u8],
    overlap: OverlapLowering,
    run: impl FnOnce(&mut IrProgram) -> R,
) -> R {
    with_ir_layout(
        source,
        overlap,
        TargetLayout::host().expect("supported test target"),
        run,
    )
}

pub(super) fn with_ir_layout<R>(
    source: &[u8],
    overlap: OverlapLowering,
    target: TargetLayout,
    run: impl FnOnce(&mut IrProgram) -> R,
) -> R {
    let inputs = [SourceInput::new("test.wf", source)];
    let bundle = SourceBundle::with_prelude(&inputs, SOURCE_LIMITS).expect("valid test bundle");
    let LexOutcome::Complete(lexed) = lex(&bundle, LEX_LIMITS) else {
        panic!("ordinary ABI test source must lex");
    };
    let TerminalOutcome::Complete(classified) = classify_terminals(
        &lexed,
        ACTIVE_KERNEL_SPEC_HASH,
        TerminalLimits {
            max_tokens: LEX_LIMITS.max_tokens,
        },
    ) else {
        panic!("ordinary ABI test source must classify");
    };
    let ParseOutcome::Complete(parsed) = parse(classified, PARSE_LIMITS) else {
        panic!("ordinary ABI test source must parse");
    };
    let FinalizeOutcome::Complete(finalized) = finalize(parsed, FINALIZE_LIMITS) else {
        panic!("ordinary ABI test source must finalize");
    };
    let CanonicalOutcome::Complete(canonical) = audit_canonical(*finalized, CANONICAL_LIMITS)
    else {
        panic!("ordinary ABI test source must be canonical");
    };
    let ResolutionOutcome::Complete(resolved) = resolve(canonical) else {
        panic!("ordinary ABI test source must resolve");
    };
    let checked = match check_semantics(&resolved) {
        SemanticOutcome::Complete(checked) => checked,
        other => panic!("ordinary ABI test source must check: {other:?}"),
    };
    let mut ir = lower_checked_with_layout(*checked, overlap, target)
        .expect("checked program must lower for the selected target");
    run(&mut ir)
}

pub(super) fn corpus_source(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/conformance/cases")
            .join(format!("{name}.wf")),
    )
    .expect("retained ordinary corpus source")
}

#[test]
fn ordinary_declarations_have_no_frame_and_share_the_call_abi() {
    with_ir(
        br#"fn relay(code: u8) -> result: std::process::ExitStatus pure {
  return std::process::exit_status(code: code);
}
"#,
        |program| {
            let declared = program
                .functions()
                .iter()
                .find(|function| function.name() == "std.process.exit_status")
                .expect("the standard library signature");
            assert!(declared.blocks().is_empty());
            let plan = crate::backend::storage::FunctionStoragePlan::build(program, declared)
                .expect("a declaration has no activation to allocate");
            assert!(plan.slots().is_empty());
            let llvm = crate::emit_llvm(program)
                .expect("ordinary signature and body emit")
                .into_string();
            assert!(
                llvm.contains("declare void @wf_std.process.exit_status(ptr %wf.result, i8 %v0)")
            );
            assert!(llvm.contains("call void @wf_std.process.exit_status(ptr"));
            assert!(
                !llvm.contains("@main("),
                "a callable unit needs no selected entry"
            );
        },
    );
}

/// The same declared boundary `host_copy_bytes` carries [PRE-2], written as
/// an ordinary Whitefoot definition.
///
/// The writer-side clauses use the local binder `copied` where the library
/// record uses `next`. Both spellings are ordinary identifiers; renaming the
/// binder changes neither the declared relation nor the ABI being compared.
const COPY_BYTES_WRAPPER: &str = r#"fn copy_bytes(value: &std::text::HostString, destination: &[u8], start: u64, end: u64) -> result: Result<u64, std::text::CopyError> reads(value), writes(destination) contract {
  requires start <= end;
  requires end <= deref(destination).len;
  ensures when Ok(value: copied): start <= copied;
  ensures when Ok(value: copied): copied <= end;
} {
  match std::text::host_copy_bytes(value: value, destination: destination, start: start, end: end) {
    Ok(value: copied) => {
      return Ok<u64, std::text::CopyError>(value: copied);
    }
    Err(error: problem) => {
      return Err<u64, std::text::CopyError>(error: problem);
    }
  }
}

"#;

#[test]
fn a_range_reference_signature_is_identical_for_a_wf_body_and_a_linked_body() {
    let original = String::from_utf8(corpus_source("run-syshost-nontext-argv-bytes-roundtrip"))
        .expect("source is UTF-8");
    // The wrapper follows the corpus source, whose alias header leads its
    // record [MOD-4].
    let source = format!(
        "{}\n\n{}\n",
        original
            .replace("host_copy_bytes(", "copy_bytes(")
            .trim_end(),
        COPY_BYTES_WRAPPER.trim_end()
    );
    with_ir(source.as_bytes(), |program| {
        let signature = |name| {
            let function = program
                .functions()
                .iter()
                .find(|function| function.name() == name)
                .expect("ordinary function exists");
            crate::backend::abi::FunctionAbi::build(program, function).expect("one callable ABI")
        };
        assert_eq!(
            signature("copy_bytes"),
            signature("std.text.host_copy_bytes")
        );
    });
    // Exercise each supported command-line configuration through its real
    // driver API: default, explicit --no-overlap, and --par. The first two
    // select Off today; neither is an arithmetic-proof acceptance switch.
    for overlap in [None, Some(OverlapLowering::Off), Some(OverlapLowering::On)] {
        let inputs = [SourceInput::new("test.wf", source.as_bytes())];
        let llvm = match overlap {
            None => crate::compile(&inputs, crate::CompilerLimits::default()),
            Some(overlap) => {
                crate::compile_with_overlap(&inputs, crate::CompilerLimits::default(), overlap)
            }
        };
        let llvm = llvm.expect("ordinary range-reference wrapper compiles in each driver mode");
        let result = compile_and_run_with(&llvm, &[b"a\xffb"]);
        assert!(
            result.status.success(),
            "same-signature range-reference call: {result:?}"
        );
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn behavior_actuals_preserve_ordinary_range_reference_calls_rows_and_contracts() {
    // The interface uses the same locally renamed clause binder as the
    // ordinary wrapper; the relation and callable boundary stay identical.
    let formal = r#"interface Copier {
  fn transfer(value: &std::text::HostString, destination: &[u8], start: u64, end: u64) -> result: Result<u64, std::text::CopyError> reads(value), writes(destination) contract {
    requires start <= end;
    requires end <= deref(destination).len;
    ensures when Ok(value: copied): start <= copied;
    ensures when Ok(value: copied): copied <= end;
  };
}

"#;
    let forwarding = COPY_BYTES_WRAPPER
        .replacen("fn copy_bytes(", "fn copy_through<interface Copier>(", 1)
        .replace("std::text::host_copy_bytes(", "Copier::transfer(");
    let original = String::from_utf8(corpus_source("run-syshost-nontext-argv-bytes-roundtrip"))
        .expect("source is UTF-8");
    let caller = original.replace("host_copy_bytes(", "copy_through::<Selected>(");
    // Each member names its linked symbol; the declarations follow the
    // caller, whose alias header leads its record [MOD-4].
    for (member, symbol) in [
        ("std::text::host_copy_bytes", "wf_std.text.host_copy_bytes"),
        ("copy_bytes", "wf_copy_bytes"),
    ] {
        let actual = format!("binding Selected : Copier {{\n  transfer = {member};\n}}\n\n");
        let suffix = format!("{formal}{actual}{COPY_BYTES_WRAPPER}{forwarding}");
        let source = format!("{}\n\n{}\n", caller.trim_end(), suffix.trim_end());
        let forwarding_name = with_ir(source.as_bytes(), |program| {
            let names = program
                .functions()
                .iter()
                .filter(|function| function.name().starts_with("copy_through$instance$"))
                .map(|function| function.name().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(names.len(), 1, "one concrete forwarding instance");
            names[0].clone()
        });
        for overlap in [None, Some(OverlapLowering::Off), Some(OverlapLowering::On)] {
            let inputs = [SourceInput::new("test.wf", source.as_bytes())];
            let llvm = match overlap {
                None => crate::compile(&inputs, crate::CompilerLimits::default()),
                Some(overlap) => {
                    crate::compile_with_overlap(&inputs, crate::CompilerLimits::default(), overlap)
                }
            }
            .expect("WF and linked actuals compile through the ordinary call path");
            let body = emitted_function(&llvm, &forwarding_name);
            assert!(body.contains(&format!("call void @{symbol}(")), "{body}");
            let output = compile_and_run_with(&llvm, &[b"a\xffb"]);
            assert!(output.status.success(), "{member}: {output:?}");
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }

        let out_of_range = format!(
            "{}\n\n{}\n",
            caller.replace("end: 4_u64", "end: 5_u64").trim_end(),
            suffix.trim_end()
        );
        assert_eq!(
            compile_rejection(out_of_range.as_bytes()).rule_id(),
            Some("FN-8")
        );
        // Two directions [FN-4] refuses: an actual whose row writes a path the
        // formal does not declare, and an actual whose `requires` is stronger
        // than the formal's. Dropping an `ensures` from the formal is the
        // third direction and is *admitted* — [FN-4] asks the actual's
        // postcondition to be stronger, and an actual carrying one clause the
        // formal does not is exactly that — so the second negative narrows a
        // `requires` instead of an `ensures`.
        for changed_formal in [
            formal.replace(", writes(destination)", ""),
            formal.replace("    requires end <= deref(destination).len;\n", ""),
        ] {
            // FN-4 checks the binding itself, before any generic caller is
            // needed. Keeping the forwarding body out of these negatives
            // avoids its separate row or postcondition failure under the
            // deliberately narrowed formal boundary.
            let mismatched = format!("{changed_formal}{actual}{COPY_BYTES_WRAPPER}");
            let mismatched = format!("{}\n", mismatched.trim_end());
            let failure = compile_rejection(mismatched.as_bytes());
            assert_eq!(failure.rule_id(), Some("FN-4"), "{member}: {failure:?}");
        }
    }
}

#[test]
fn an_entry_selecting_no_input_starts_and_returns_its_status() {
    let llvm = compile(
        br#"fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 37_u8);
}
"#,
    );
    let output = compile_and_run(&llvm);
    assert_eq!(output.status.code(), Some(37));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn opaque_drop_has_no_implicit_native_close() {
    let llvm = compile(
        br#"fn main() -> status: std::process::ExitStatus pure {
  let unused = std::process::exit_status(code: 9_u8);
  return std::process::exit_status(code: 0_u8);
}
"#,
    );
    assert!(!llvm.contains("call void @wf_std.fs.close_"));
    assert!(compile_and_run(&llvm).status.success());
}
