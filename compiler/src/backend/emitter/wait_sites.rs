//! Immutable runtime wait descriptions, retained by each using link fragment.

use super::{FunctionEmitter, Module, llvm_bytes};
use crate::{IrInstruction, IrOperation, IrProgram, IrValueId};

fn symbol(function: &str, result: IrValueId) -> String {
    format!(".wf_wait.{function}.{}", result.ordinal())
}

pub(super) fn emit_wait_sites(output: &mut Module, program: &IrProgram) {
    for function in program.functions() {
        for instruction in function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
        {
            let IrInstruction::Define {
                result, operation, ..
            } = instruction
            else {
                continue;
            };
            let site = match operation {
                IrOperation::ContextJoin { site }
                | IrOperation::ContextAwait { site, .. }
                | IrOperation::SharedWatch { site, .. }
                | IrOperation::WatchPark { site, .. } => site,
                _ => continue,
            };
            let mut bytes = site.as_bytes().to_vec();
            bytes.push(0);
            output.fragment_local_constant(
                symbol(function.name(), *result),
                format!("[{} x i8]", bytes.len()),
                format!("c\"{}\"", llvm_bytes(&bytes)),
                Some(1),
            );
        }
    }
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn wait_site_operand(&mut self, result: IrValueId) -> String {
        let name = symbol(self.function.name(), result);
        self.output.symbol(name.clone());
        format!("@{name}")
    }
}
