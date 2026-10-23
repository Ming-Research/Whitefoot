//! Placement predictions, never facts used for acceptance or optimization
//! legality. See compiler/branch-weights and the halo-call-path study's S1–S3.

use std::collections::{HashMap, HashSet, VecDeque};

use super::dispatch;
use crate::{
    IrBlockId, IrEnumType, IrFunction, IrInstruction, IrOperation, IrProgram, IrTerminator, IrType,
};

const STRUCTURAL_LIKELY_WEIGHT: u32 = 2000;
/// Kept separate so call predictions can be reconsidered independently.
pub(super) const CALL_LIKELY_WEIGHT: u32 = 2000;
/// A general pre-inlining proxy, not a claim about the host's inline budget.
/// Tiny bodies contribute only calls that survive recursively through them;
/// intrinsics are separate IR operations and never seed this prediction.
const INLINE_INSTRUCTION_LIMIT: usize = 16;

pub(super) struct BranchWeights {
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    result_nominals: HashSet<usize>,
    returns_error: Vec<bool>,
    returns: Vec<bool>,
    has_call: Vec<bool>,
}

impl BranchWeights {
    pub(super) fn new(program: &IrProgram, function: &IrFunction) -> Self {
        // This is the retained canonical prelude identity, not a source
        // function/name heuristic. User nominal identities are qualified;
        // the semantic prelude assigns Result's Ok/Err tags 0/1.
        let result_nominals: HashSet<_> = program
            .nominals()
            .iter()
            .filter(|nominal| {
                nominal
                    .stable_spelling()
                    .is_some_and(|name| name.starts_with("Result<"))
            })
            .map(|nominal| nominal.id().index())
            .collect();
        let error_values: HashSet<_> = function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .filter_map(|instruction| match instruction {
                IrInstruction::Define {
                    result,
                    operation:
                        IrOperation::ConstructEnum {
                            nominal,
                            variant: 1,
                            ..
                        },
                    ..
                } if function.result() == IrType::Nominal(*nominal)
                    && result_nominals.contains(&nominal.index()) =>
                {
                    Some(*result)
                }
                _ => None,
            })
            .collect();
        // The dispatch planner's edge list, so a new terminator kind cannot
        // reach one analysis and miss the other.
        let successors: Vec<Vec<_>> = function.blocks().iter().map(dispatch::successors).collect();
        let mut predecessors = vec![Vec::new(); successors.len()];
        for (from, targets) in successors.iter().enumerate() {
            for &to in targets {
                if let Some(incoming) = predecessors.get_mut(to) {
                    incoming.push(from);
                }
            }
        }
        let mut memo = HashMap::new();
        let mut active = HashSet::new();
        let has_call =
            function
                .blocks()
                .iter()
                .map(|block| {
                    block.instructions().iter().enumerate().any(|(at, instruction)| {
                    let IrInstruction::Define {
                        result,
                        operation: IrOperation::Call { function: callee, .. },
                        ..
                    } = instruction else {
                        return false;
                    };
                    // Self-tail transfers already are Jump, and dispatch
                    // successor transfers are synthesized after this analysis.
                    // Also exclude an ordinary call in direct tail position.
                    let tail = at + 1 == block.instructions().len()
                        && matches!(block.terminator(), IrTerminator::Return { value, drops }
                            if value == result && drops.is_empty());
                    !tail && call_survives(program, *callee as usize, &mut memo, &mut active)
                })
                })
                .collect();
        let returns = function
            .blocks()
            .iter()
            .map(|block| matches!(block.terminator(), IrTerminator::Return { .. }))
            .collect();
        let returns_error = function
            .blocks()
            .iter()
            .map(|block| {
                matches!(block.terminator(), IrTerminator::Return { value, .. }
                if error_values.contains(value))
            })
            .collect();
        let mut analysis = Self {
            successors,
            predecessors,
            result_nominals,
            returns_error,
            returns,
            has_call,
        };
        analysis.returns_error = analysis.all_paths(&analysis.returns_error, None, false);
        analysis
    }

    /// Weights in IR target order. The emitter supplies the unreachable
    /// switch default separately; these predictions never give it a path.
    pub(super) fn for_match(
        &self,
        function: &IrFunction,
        block: IrBlockId,
        split_header: Option<IrBlockId>,
        calls: bool,
    ) -> Option<Vec<u32>> {
        let IrTerminator::Match {
            enum_type, targets, ..
        } = function.blocks().get(block.index())?.terminator()
        else {
            return None;
        };
        let result_match = matches!(enum_type, IrEnumType::Nominal(id)
            if self.result_nominals.contains(&id.index()));
        let header = split_header.map(IrBlockId::index);
        // S2: finite paths that return without visiting the restored header.
        let exits = header.map(|header| self.all_paths(&self.returns, Some(header), false));
        let unlikely: Vec<_> = targets
            .iter()
            .map(|target| {
                (result_match && target.tag() == 1)
                    || self.returns_error[target.block().index()]
                    || exits
                        .as_ref()
                        .is_some_and(|exits| exits[target.block().index()])
            })
            .collect();
        if unlikely.iter().any(|cold| *cold) {
            // Structural evidence owns the prediction, including ties: S3
            // must not reverse Result's failure classification.
            return mixed_weights(&unlikely, STRUCTURAL_LIKELY_WEIGHT);
        }
        if !calls || targets.len() != 2 || targets[0].block() == targets[1].block() {
            return None;
        }
        let regions: Vec<_> = targets
            .iter()
            .map(|target| self.reachable(target.block().index(), header))
            .collect();
        // LLVM retains ordinary loops. Only the selected dispatch header
        // disappears; inner loops still suppress this call prediction.
        if regions.iter().any(|region| region[block.index()]) {
            return None;
        }
        let mut endpoints = self.returns.clone();
        if let Some(header) = header {
            endpoints[header] = true;
        }
        let call_free = self.all_paths(&endpoints, header, true);
        let contains_call: Vec<_> = regions
            .iter()
            .map(|region| {
                region
                    .iter()
                    .zip(&self.has_call)
                    .any(|(member, call)| *member && *call)
            })
            .collect();
        // An entirely call-free path to return/header witnesses that a call
        // on the other side cannot post-dominate this branch. Requiring all
        // paths on the opposite side to be call-free also rejects shared
        // calls and ties, without an all-pairs post-dominance matrix.
        let cold = [
            contains_call[0] && call_free[targets[1].block().index()],
            contains_call[1] && call_free[targets[0].block().index()],
        ];
        mixed_weights(&cold, CALL_LIKELY_WEIGHT)
    }

    /// Least fixed point of finite all-successor paths. Cycles and
    /// Unreachable do not manufacture return evidence. Each edge is visited
    /// once, including repeated switch destinations.
    fn all_paths(&self, seeds: &[bool], header: Option<usize>, no_calls: bool) -> Vec<bool> {
        let mut accepted = vec![false; self.successors.len()];
        let mut remaining: Vec<_> = self.successors.iter().map(Vec::len).collect();
        let mut pending = VecDeque::new();
        for (block, seed) in seeds.iter().enumerate() {
            if *seed && (Some(block) == header || !no_calls || !self.has_call[block]) {
                accepted[block] = true;
                pending.push_back(block);
            }
        }
        while let Some(block) = pending.pop_front() {
            for &predecessor in &self.predecessors[block] {
                if accepted[predecessor]
                    || Some(predecessor) == header
                    || (no_calls && self.has_call[predecessor])
                {
                    continue;
                }
                remaining[predecessor] -= 1;
                if remaining[predecessor] == 0 {
                    accepted[predecessor] = true;
                    pending.push_back(predecessor);
                }
            }
        }
        accepted
    }

    fn reachable(&self, start: usize, header: Option<usize>) -> Vec<bool> {
        let mut visited = vec![false; self.successors.len()];
        let mut pending = vec![start];
        while let Some(block) = pending.pop() {
            if Some(block) == header || visited[block] {
                continue;
            }
            visited[block] = true;
            pending.extend(self.successors[block].iter().copied());
        }
        visited
    }
}

fn mixed_weights(unlikely: &[bool], likely: u32) -> Option<Vec<u32>> {
    (unlikely.iter().any(|cold| *cold) && unlikely.iter().any(|cold| !*cold)).then(|| {
        unlikely
            .iter()
            .map(|cold| if *cold { 1 } else { likely })
            .collect()
    })
}

/// Large or external synchronous bodies count as surviving calls. A tiny
/// wrapper contributes its inner surviving calls even if the wrapper itself
/// inlines; a recursive cycle cannot disappear by recursively inlining it.
/// Waiting callees emit resumable transfers, not the ordinary call we predict.
fn call_survives(
    program: &IrProgram,
    ordinal: usize,
    memo: &mut HashMap<usize, bool>,
    active: &mut HashSet<usize>,
) -> bool {
    if let Some(answer) = memo.get(&ordinal) {
        return *answer;
    }
    let Some(function) = program.functions().get(ordinal) else {
        return false;
    };
    if function.waits() {
        memo.insert(ordinal, false);
        return false;
    }
    if !active.insert(ordinal) {
        return true;
    }
    let answer = function.blocks().is_empty()
        || function
            .blocks()
            .iter()
            .map(|block| block.instructions().len())
            .sum::<usize>()
            > INLINE_INSTRUCTION_LIMIT
        || function
            .blocks()
            .iter()
            .flat_map(|block| block.instructions())
            .any(|instruction| {
                matches!(instruction, IrInstruction::Define {
                operation: IrOperation::Call { function, .. }, ..
            } if call_survives(program, *function as usize, memo, active))
            });
    active.remove(&ordinal);
    memo.insert(ordinal, answer);
    answer
}
