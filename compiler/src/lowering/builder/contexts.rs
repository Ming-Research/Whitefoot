//! [PAR-4] a `mustpar` statement whose callee waits, lowered to a started
//! context over a synthesized wrapper, and the join each exit of the starting
//! activation owes.
//!
//! The wrapper takes the call's arguments as parameters, makes the one call
//! and releases its result exactly as the statement would have [OWN-1]: the
//! checker admitted the start only for value parameters and a droppable
//! result, so nothing the wrapper holds is borrowed from its starter. The
//! starting block evaluates the arguments where the statement stands, which
//! is where their moves and copies take effect, and hands the values to the
//! start; the wrapper runs later, in its own context.

use crate::semantic::{CheckedExpression, CheckedProjectedDrop};
use crate::{
    IrConstant, IrOperation, IrTerminator, IrType, IrValueId, LoweringFailure, NodePath,
};

use super::IrBuilder;

impl IrBuilder<'_> {
    /// Whether the statement at `node_path` starts a context.
    pub(super) fn starts_context(&self, node_path: &NodePath) -> bool {
        self.context_starts.contains(node_path)
    }

    /// Before an exit of an activation that starts contexts, wait for them
    /// [PAR-4]. Every return, propagated error and self transfer calls this.
    pub(super) fn join_contexts(&mut self) -> Result<(), LoweringFailure> {
        if !self.context_starts.is_empty() {
            self.define(IrType::Unit, IrOperation::ContextJoin)?;
        }
        Ok(())
    }

    /// Lowers one context-starting statement: its call's arguments here, and
    /// the call with its release in a new wrapper the start names.
    pub(super) fn start_context(
        &mut self,
        expression: &CheckedExpression,
        drops: &[CheckedProjectedDrop],
    ) -> Result<(), LoweringFailure> {
        let CheckedExpression::UserCall {
            call, arguments, ..
        } = expression
        else {
            return Err(LoweringFailure::InvalidCheckedProgram);
        };
        let function = self
            .physical_calls
            .iter()
            .find_map(|(site, target)| (site == call).then_some(*target))
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let values = arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let types = values
            .iter()
            .map(|value| self.value_type(*value))
            .collect::<Result<Vec<_>, _>>()?;
        let result = *self
            .function_results
            .get(function as usize)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;

        let mut wrapper = IrBuilder::new(
            self.context(),
            IrType::Unit,
            std::collections::HashSet::new(),
            None,
            self.overlap,
            self.function_name,
        )?;
        let parameters = types
            .iter()
            .map(|ty| wrapper.new_parameter(*ty))
            .collect::<Result<Vec<IrValueId>, _>>()?;
        let returned = wrapper.define(
            result,
            IrOperation::Call {
                function,
                arguments: parameters,
            },
        )?;
        let mut lowered = Vec::with_capacity(drops.len());
        for drop in drops {
            lowered.push(wrapper.lower_projected_drop(returned, drop)?);
        }
        wrapper.append_drops(lowered)?;
        let unit = wrapper.define(IrType::Unit, IrOperation::Constant(IrConstant::Unit))?;
        wrapper.terminate(IrTerminator::Return {
            value: unit,
            drops: Vec::new(),
        })?;
        let (ordinal, name) = self.synthesis.borrow_mut().reserve(self.function_name)?;
        let mut function = wrapper.finish(context_symbol(&name), Vec::new(), None)?;
        function.name = context_symbol(&name);
        self.synthesis.borrow_mut().file(ordinal, function)?;

        self.define(
            IrType::Unit,
            IrOperation::ContextStart {
                function: ordinal,
                arguments: values,
            },
        )?;
        Ok(())
    }
}

/// The wrapper's symbol stem. `wf_` precedes it and FORM-3 starts no IDENT
/// with an underscore, so no source function can collide with it.
fn context_symbol(name: &str) -> String {
    format!("_ctx_start_{name}")
}
