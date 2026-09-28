//! The bundled executable runner's ordinary caller.
//!
//! This is build glue, applied after acceptance. The language does not require
//! an entry name or signature. Other signatures remain callable from linked
//! code; this runner supplies Inputs, or no arguments.

use std::fmt::Write;

use crate::backend::abi::{FunctionAbi, ParameterAbi, ResultAbi};
use crate::backend::emission::{FunctionBody, Module, Parameter, Signature};
use crate::backend::emitter::source_symbol;
use crate::{BackendFailure, IrNominalKind, IrProgram, IrSourceMode, IrType};

/// The standard library's invocation inputs and exit status, by the
/// module-qualified spelling that names them apart from any program type of
/// the same name [PRE-2, MOD-10].
const INPUTS: &str = "std.process.Inputs";
const EXIT_STATUS: &str = "std.process.ExitStatus";

pub(crate) fn render(program: &IrProgram, selected: &str) -> Result<Module, BackendFailure> {
    let Some(main) = program
        .functions()
        .iter()
        .find(|function| function.name() == selected)
    else {
        return Ok(Module::default());
    };
    let Some(signature) = main.source_signature() else {
        return Ok(Module::default());
    };
    if signature.result() != IrSourceMode::Own
        || signature
            .parameters()
            .iter()
            .any(|mode| *mode != IrSourceMode::Own)
    {
        return Ok(Module::default());
    }
    let abi = FunctionAbi::build(program, main)?;
    let mut arguments = Vec::new();
    let mut inputs = None;
    for parameter in abi.parameters() {
        match parameter {
            ParameterAbi::ContentPointer(IrType::Nominal(id))
                if inputs.is_none()
                    && program
                        .nominal(*id)
                        .is_some_and(|nominal| nominal.stable_spelling() == Some(INPUTS)) =>
            {
                inputs = Some(*id);
                arguments.push("ptr %inputs".to_owned());
            }
            _ => return Ok(Module::default()),
        }
    }
    let status = match abi.result() {
        ResultAbi::Destination(IrType::Nominal(id))
            if program.nominal(id).is_some_and(|nominal| {
                nominal.stable_spelling() == Some(EXIT_STATUS)
                    && matches!(nominal.kind(), IrNominalKind::Opaque)
            }) =>
        {
            Some(id)
        }
        ResultAbi::Value(IrType::Unit) => None,
        _ => return Ok(Module::default()),
    };
    let mut module = Module::default();
    if inputs.is_some() {
        module.text("\n");
        module.declare(Signature::new(
            "wf__ordinary_inputs",
            "i32",
            vec![
                Parameter::unnamed("ptr"),
                Parameter::unnamed("i32"),
                Parameter::unnamed("ptr"),
            ],
        ));
    }
    if status.is_some() {
        module.text("\n");
        module.declare(Signature::new(
            "wf__ordinary_exit_code",
            "i8",
            vec![Parameter::unnamed("ptr")],
        ));
    }
    module.text("\n");
    let signature = Signature::new(
        "wf__main_body",
        "i32",
        vec![
            Parameter::named("i32", "%argc"),
            Parameter::named("ptr", "%argv"),
        ],
    );
    let mut output = FunctionBody::default();
    output.open_block("entry".to_owned());
    if let Some(id) = inputs {
        let ty = output.type_name(program, IrType::Nominal(id))?;
        writeln!(output, "  %inputs = alloca {ty}, align 16")
            .map_err(|_| BackendFailure::TextEmission)?;
        output.instructions("  %ready = call i32 @wf__ordinary_inputs(ptr %inputs, i32 %argc, ptr %argv)\n  %ok = icmp ne i32 %ready, 0\n  br i1 %ok, label %invoke, label %unavailable\n", &["wf__ordinary_inputs"]);
        output.open_block("unavailable".to_owned());
        output.push_str("  ret i32 70\n");
        output.open_block("invoke".to_owned());
    }
    if let Some(id) = status {
        let ty = output.type_name(program, IrType::Nominal(id))?;
        writeln!(output, "  %status = alloca {ty}, align 16")
            .map_err(|_| BackendFailure::TextEmission)?;
        arguments.insert(0, "ptr %status".to_owned());
    }
    let callee = source_symbol(main.name());
    // A waiting entry [WAIT-1] is a resumable frame: the launcher makes it in
    // the root context with the no-op coroutine as its parent, and the root
    // context runs every context until that frame has finished
    // (design/amendments/compiler-waiting-contexts.md). Its result is always
    // constructed through a destination, a unit one included.
    let waiting = main.waits();
    if waiting {
        if status.is_none() {
            output.push_str("  %unit.slot = alloca i8, align 1\n");
            arguments.insert(0, "ptr %unit.slot".to_owned());
        }
        arguments.insert(1, "ptr %wf.parent".to_owned());
        output.instructions(
            "  call void @wf__context_root_begin()\n  %wf.parent = call ptr @llvm.coro.noop()\n",
            &["wf__context_root_begin", "llvm.coro.noop"],
        );
    }
    let result_type = if status.is_some() { "void" } else { "i8" };
    // One call of the selected entry or of its sequential clone, named apart
    // by `suffix` where both are emitted.
    let entry_call = |output: &mut FunctionBody, symbol: &str, suffix: &str| {
        output.symbol(symbol);
        if waiting {
            output.symbol("wf__context_root_run");
            writeln!(
                output,
                "  %wf.root{suffix} = call ptr @\"{symbol}\"({})\n  call void @wf__context_root_run(ptr %wf.root{suffix})",
                arguments.join(", ")
            )
        } else {
            let assignment = if status.is_some() {
                String::new()
            } else {
                format!("%unit{suffix} = ")
            };
            writeln!(
                output,
                "  {assignment}call {result_type} @\"{symbol}\"({})",
                arguments.join(", ")
            )
        }
        .map_err(|_| BackendFailure::TextEmission)
    };
    if let Some(sequential) =
        crate::backend::emitter::sequential_entry_symbol(program, main.name())?
    {
        output.instructions("  %par.pool = call i32 @wf__par_pool_active()\n  %par.active = icmp ne i32 %par.pool, 0\n  br i1 %par.active, label %parallel, label %sequential\n", &["wf__par_pool_active"]);
        output.open_block("parallel".to_owned());
        entry_call(&mut output, &callee, ".par")?;
        output.push_str("  br label %returned\n");
        output.open_block("sequential".to_owned());
        entry_call(&mut output, &sequential, ".seq")?;
        output.push_str("  br label %returned\n");
        output.open_block("returned".to_owned());
    } else {
        entry_call(&mut output, &callee, "")?;
    }
    if status.is_some() {
        output.instructions("  %code = call i8 @wf__ordinary_exit_code(ptr %status)\n  %exit = zext i8 %code to i32\n  ret i32 %exit\n", &["wf__ordinary_exit_code"]);
    } else {
        output.push_str("  ret i32 0\n");
    }
    module.define(signature.define(output, "")?);
    module.text("\n");
    let signature = Signature::new(
        "main",
        "i32",
        vec![
            Parameter::named("i32", "%argc"),
            Parameter::named("ptr", "%argv"),
        ],
    );
    let mut output = FunctionBody::default();
    output.open_block("entry".to_owned());
    output.instructions(
        "  %status = call i32 @wf__floor_run(i32 %argc, ptr %argv)\n  ret i32 %status\n",
        &["wf__floor_run"],
    );
    module.define(signature.define(output, "")?);
    Ok(module)
}

/// Constructs the executable builder's caller as ordinary WF source. Copying
/// the selected declaration's header preserves its types, brands and exact
/// row; omitting its contract makes CALL-6 prove the call from parameter type
/// facts alone. A failed proof leaves a callable library module, not a runtime
/// precondition test or an acceptance exception for the selected function.
pub(super) fn caller_source(
    resolved: &crate::ResolvedSyntaxUnit,
    function: &crate::semantic::CheckedFunction,
) -> Option<(String, String)> {
    use crate::Production;
    use crate::syntax::{FinalizedExtent, NodeId};

    let selected = function.name.as_str();
    let origin = resolved
        .declaration(function.declaration)?
        .origin()
        .coordinate();
    let syntax = resolved.syntax();
    let tree = &syntax.finalized.topology;
    let (index, node) = tree.nodes.iter().enumerate().find(|(_, node)| {
        node.production == Production::FnDecl
            && matches!(node.extent, FinalizedExtent::Source {source, start, end}
                if source == origin.source() && start <= origin.start() && origin.end() <= end)
    })?;
    let children = tree.node_children(NodeId::from_index(index)?)?;
    if children.iter().any(|child| {
        tree.node(*child)
            .is_some_and(|node| node.production == Production::Generics)
    }) {
        return None;
    }
    let effects = children.iter().find_map(|child| {
        tree.node(*child)
            .filter(|node| node.production == Production::Effects)
    })?;
    let FinalizedExtent::Source { source, start, .. } = node.extent else {
        return None;
    };
    let FinalizedExtent::Source { end, .. } = effects.extent else {
        return None;
    };
    let bytes = syntax
        .classified_bundle()
        .source_bundle()
        .file(source)?
        .bytes();
    let start = usize::try_from(start.value()).ok()?;
    let end = usize::try_from(end.value()).ok()?;
    let mut header = std::str::from_utf8(bytes.get(start..end)?).ok()?.to_owned();
    // The copied header names types through the selected record's aliases,
    // which bind in that record alone [MOD-4], so the caller repeats them.
    let mut aliases = String::new();
    for node in &tree.nodes {
        if node.production != Production::AliasDecl {
            continue;
        }
        if let FinalizedExtent::Source {
            source: alias_source,
            start: alias_start,
            end: alias_end,
        } = node.extent
            && alias_source == source
        {
            let alias_start = usize::try_from(alias_start.value()).ok()?;
            let alias_end = usize::try_from(alias_end.value()).ok()?;
            aliases.push_str(std::str::from_utf8(bytes.get(alias_start..alias_end)?).ok()?);
            aliases.push('\n');
        }
    }
    if !aliases.is_empty() {
        aliases.push('\n');
    }
    let suffix = (0_u64..).find(|index| {
        let name = format!("executable_caller_{index}");
        !resolved.declarations().iter().any(|declaration| {
            declaration.spelling() == name
                || declaration.spelling() == format!("executable_value_{index}")
        })
    })?;
    let name = format!("executable_caller_{suffix}");
    // fn_decl's first two terminals are `fn` and its declared IDENT. The
    // canonical header has no trivia ambiguity and no contract is retained.
    header.replace_range(3..3 + selected.len(), &name);
    let arguments = function
        .parameters
        .iter()
        .map(|parameter| format!("{}: move {}", parameter.name, parameter.name))
        .collect::<Vec<_>>()
        .join(", ");
    let transfer = if function.result == crate::CheckedType::Unit {
        ""
    } else {
        "move "
    };
    Some((
        name,
        format!(
            "{aliases}{header} {{\n  let executable_value_{suffix} = {selected}({arguments});\n  return {transfer}executable_value_{suffix};\n}}\n"
        ),
    ))
}
