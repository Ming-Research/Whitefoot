//! [WAIT-3] a spawn statement, lowered to a started context over a
//! synthesized wrapper, and the joins it owes.
//!
//! The wrapper takes the call's arguments as parameters and makes the one
//! call. For an expression statement it releases the result exactly as the
//! statement would have [OWN-1], and the activation joins every such context
//! before each of its exits. For a `let` it returns the result, which the
//! context constructs in a slot of the starting frame, and the binding is
//! defined by an await the checker's plan places before the binding's first
//! use or its block's end [WAIT-3]. The checker admitted the start only for
//! value parameters, so nothing the wrapper holds is borrowed from its
//! starter. The starting block evaluates the arguments where the statement
//! stands, which is where their moves and copies take effect, and hands the
//! values to the start; the wrapper runs later, in its own context.

use crate::semantic::{BindingId, CheckedExpression, CheckedProjectedDrop};
use crate::{IrConstant, IrOperation, IrTerminator, IrType, IrValueId, LoweringFailure, NodePath};

use super::IrBuilder;

impl IrBuilder<'_> {
    /// Whether the statement at `node_path` starts a context.
    pub(super) fn starts_context(&self, node_path: &NodePath) -> bool {
        self.context_starts.contains(node_path)
    }

    /// Before an exit of an activation that starts contexts, wait for them
    /// [WAIT-3]. Every return, propagated error and self transfer calls this.
    /// The plan awaits every bound context before any statement that may
    /// leave its block, so none is pending here; one that were would be
    /// awaited here rather than outlived by the frame its result lands in.
    pub(super) fn join_contexts(&mut self) -> Result<(), LoweringFailure> {
        let pending = self
            .pending_contexts
            .iter()
            .map(|pending| pending.start)
            .collect::<Vec<_>>();
        for start in pending {
            let ty = self.bound_result_type(start)?;
            self.define(ty, IrOperation::ContextAwait { start })?;
        }
        if !self.context_starts.is_empty() {
            self.define(IrType::Unit, IrOperation::ContextJoin)?;
        }
        Ok(())
    }

    /// Awaits every bound context of the innermost block, started at or after
    /// `from` in the pending stack, whose plan joins it before the statement
    /// at `index`, and defines its binding [WAIT-3].
    pub(super) fn await_contexts_before(
        &mut self,
        from: usize,
        index: usize,
    ) -> Result<(), LoweringFailure> {
        let mut position = from;
        while position < self.pending_contexts.len() {
            let due = self.pending_contexts[position]
                .before
                .is_none_or(|before| before <= index);
            if !due {
                position += 1;
                continue;
            }
            let pending = self.pending_contexts.remove(position);
            let value = self.define(
                pending.result,
                IrOperation::ContextAwait {
                    start: pending.start,
                },
            )?;
            if self.bindings.insert(pending.binding, value).is_some() {
                return Err(LoweringFailure::InvalidCheckedProgram);
            }
            self.promote_binding_if_needed(pending.binding)?;
        }
        Ok(())
    }

    fn bound_result_type(&self, start: IrValueId) -> Result<IrType, LoweringFailure> {
        self.pending_contexts
            .iter()
            .find(|pending| pending.start == start)
            .map(|pending| pending.result)
            .ok_or(LoweringFailure::InvalidCheckedProgram)
    }

    /// Lowers one bound spawn at `index` of its block: its call's
    /// arguments here, the call in a wrapper that returns its result, and a
    /// pending await the plan places [WAIT-3].
    pub(super) fn start_bound_context(
        &mut self,
        node_path: &NodePath,
        binding: BindingId,
        expression: &CheckedExpression,
        index: usize,
    ) -> Result<(), LoweringFailure> {
        let before = self
            .context_awaits
            .iter()
            .find(|(statement, _)| statement == node_path)
            .map(|(_, before)| *before)
            .ok_or(LoweringFailure::InvalidCheckedProgram)?;
        let (start, result) = self.start_wrapped(expression, None)?;
        self.pending_contexts.push(PendingContext {
            binding,
            start,
            result,
            before: before.map(|before| index.saturating_add(before as usize)),
        });
        Ok(())
    }

    /// Lowers one context-starting statement: its call's arguments here, and
    /// the call with its release in a new wrapper the start names.
    pub(super) fn start_context(
        &mut self,
        expression: &CheckedExpression,
        drops: &[CheckedProjectedDrop],
    ) -> Result<(), LoweringFailure> {
        self.start_wrapped(expression, Some(drops)).map(|_| ())
    }

    /// Evaluates a started call's arguments where its statement stands and
    /// starts a context over a synthesized wrapper that makes the call. With
    /// `drops` the wrapper releases the result and returns unit; without, it
    /// returns the result, which the starting frame keeps. Returns the start
    /// and the wrapper's result type.
    fn start_wrapped(
        &mut self,
        expression: &CheckedExpression,
        drops: Option<&[CheckedProjectedDrop]>,
    ) -> Result<(IrValueId, IrType), LoweringFailure> {
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
        let wrapper_result = if drops.is_some() {
            IrType::Unit
        } else {
            result
        };

        let mut wrapper = IrBuilder::new(
            self.context(),
            wrapper_result,
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
        let value = match drops {
            Some(drops) => {
                let mut lowered = Vec::with_capacity(drops.len());
                for drop in drops {
                    lowered.push(wrapper.lower_projected_drop(returned, drop)?);
                }
                wrapper.append_drops(lowered)?;
                wrapper.define(IrType::Unit, IrOperation::Constant(IrConstant::Unit))?
            }
            None => returned,
        };
        wrapper.terminate(IrTerminator::Return {
            value,
            drops: Vec::new(),
        })?;
        let (ordinal, name) = self.synthesis.borrow_mut().reserve(self.function_name)?;
        let mut function = wrapper.finish(context_symbol(&name), Vec::new(), None)?;
        function.name = context_symbol(&name);
        // The wrapper makes the waiting call, so it is a waiting function
        // itself [WAIT-1] and lowers to a resumable frame like its callee.
        function.waits = true;
        self.synthesis.borrow_mut().file(ordinal, function)?;

        let operation = if drops.is_some() {
            IrOperation::ContextStart {
                function: ordinal,
                arguments: values,
            }
        } else {
            IrOperation::ContextStartBound {
                function: ordinal,
                arguments: values,
            }
        };
        let start = self.define(IrType::Unit, operation)?;
        Ok((start, result))
    }
}

/// [WAIT-3] one bound context started and not yet awaited.
pub(super) struct PendingContext {
    /// The binding its result defines.
    binding: BindingId,
    /// The start's value, which the await names.
    pub(super) start: IrValueId,
    /// The result's type.
    result: IrType,
    /// The index in its block of the statement it is awaited before, or
    /// `None` for the block's end.
    before: Option<usize>,
}

/// The wrapper's symbol stem. `wf_` precedes it and FORM-3 starts no IDENT
/// with an underscore, so no source function can collide with it.
fn context_symbol(name: &str) -> String {
    format!("_ctx_start_{name}")
}
