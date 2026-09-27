//! Finite physical call inventory after source acceptance.
//!
//! Semantic function identities, proof summaries, and permission tables stay
//! canonical. [STOR-8] gives the language one heap and no region parameters,
//! so each emitted source function has exactly one physical variant, and the
//! inventory is the set of functions a build emits with each call resolved to
//! its callee's variant.

use crate::NodePath;
use crate::semantic::{
    CheckedElement, CheckedFunction, CheckedProgramData, CheckedType, FunctionId, FunctionMentions,
};

use super::LoweringFailure;

#[derive(Debug)]
pub(super) struct PhysicalVariant {
    pub(super) source: FunctionId,
    pub(super) calls: Vec<(NodePath, u32)>,
}

#[derive(Debug)]
pub(super) struct PhysicalFunctions {
    pub(super) variants: Vec<PhysicalVariant>,
}

impl PhysicalFunctions {
    /// Every checked definition, each closing over its calls.
    #[cfg(test)]
    pub(super) fn build(program: &CheckedProgramData) -> Result<Self, LoweringFailure> {
        Self::build_from(program, None)
    }

    /// With `roots`, only what those functions reach through their calls:
    /// a module program entry's build [MOD-9] emits the code its run can
    /// execute and no definition outside it. Without, every checked
    /// definition.
    pub(super) fn build_from(
        program: &CheckedProgramData,
        roots: Option<&[FunctionId]>,
    ) -> Result<Self, LoweringFailure> {
        let dependencies = program
            .functions
            .iter()
            .map(FunctionMentions::collect)
            .collect::<Vec<_>>();
        debug_assert!(
            program
                .functions
                .iter()
                .all(|function| function.region_parameters.is_empty()),
            "[STOR-8] no checked function takes a region parameter"
        );
        for dependency in &dependencies {
            for call in &dependency.calls {
                source_function(program, call.callee)?;
            }
        }
        // With roots, the functions they reach through calls; without, every
        // checked definition.
        let mut emitted = vec![roots.is_none(); program.functions.len()];
        let mut pending = Vec::new();
        for root in roots.unwrap_or_default() {
            let slot = emitted
                .get_mut(root.0 as usize)
                .ok_or(LoweringFailure::InvalidCheckedProgram)?;
            if !*slot {
                *slot = true;
                pending.push(*root);
            }
        }
        while let Some(function) = pending.pop() {
            for call in &dependencies[function.0 as usize].calls {
                let slot = &mut emitted[call.callee.0 as usize];
                if !*slot {
                    *slot = true;
                    pending.push(call.callee);
                }
            }
        }
        // A variant's ordinal is its source's rank among the emitted
        // functions, so ordinals follow source order.
        let mut ordinals = vec![None; program.functions.len()];
        let mut next = 0u32;
        for (index, emit) in emitted.iter().enumerate() {
            if *emit {
                ordinals[index] = Some(next);
                next = next
                    .checked_add(1)
                    .ok_or(LoweringFailure::CounterOverflow)?;
            }
        }
        let variants = program
            .functions
            .iter()
            .zip(&dependencies)
            .enumerate()
            .filter(|(index, _)| emitted[*index])
            .map(|(index, (function, dependency))| {
                if function.id.0 as usize != index {
                    return Err(LoweringFailure::InvalidCheckedProgram);
                }
                let calls = dependency
                    .calls
                    .iter()
                    .map(|call| {
                        ordinals[call.callee.0 as usize]
                            .map(|ordinal| (call.path.clone(), ordinal))
                            .ok_or(LoweringFailure::InvalidCheckedProgram)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(PhysicalVariant {
                    source: function.id,
                    calls,
                })
            })
            .collect::<Result<Vec<_>, LoweringFailure>>()?;
        Ok(Self { variants })
    }
}

fn source_function(
    program: &CheckedProgramData,
    source: FunctionId,
) -> Result<&CheckedFunction, LoweringFailure> {
    program
        .functions
        .get(source.0 as usize)
        .filter(|function| function.id == source)
        .ok_or(LoweringFailure::InvalidCheckedProgram)
}

pub(super) fn executable_storage(
    function: &CheckedFunction,
) -> (Vec<CheckedType>, Vec<CheckedElement>) {
    let mentions = FunctionMentions::collect(function);
    (mentions.types, mentions.elements)
}
