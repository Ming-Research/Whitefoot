//! [SHARE-2] object identity cases share the ordinary storage/version machinery.
use super::*;
use crate::semantic::model::{CheckedTarget, CheckedTargetKind};

/// Enumerate equivalence relations allowed by the pairwise may-alias relation.
/// Possibility is not transitive: a class must admit every pair of members.
/// Representatives are their first target, so each partition occurs once.
fn partitions(
    aliases: &[Vec<bool>],
    representatives: &mut Vec<usize>,
    visit: &mut impl FnMut(&[usize]),
) {
    let next = representatives.len();
    if next == aliases.len() {
        visit(representatives);
        return;
    }
    for representative in 0..next {
        if representatives[representative] != representative
            || representatives.iter().enumerate().any(|(member, class)| {
                *class == representative && !aliases[next][member]
            })
        {
            continue;
        }
        representatives.push(representative);
        partitions(aliases, representatives, visit);
        representatives.pop();
    }
    representatives.push(next);
    partitions(aliases, representatives, visit);
    representatives.pop();
}

impl Walker<'_> {
    pub(super) fn atomic_statement(
        &mut self,
        mut state: State,
        targets: &[CheckedTarget],
        node: &NodePath,
        guard: Option<&CheckedExpression>,
        body: &[CheckedStatement],
        continues: bool,
    ) -> Option<State> {
        self.cite = node.clone();
        for target in targets.iter().flat_map(CheckedTarget::expressions) {
            let _ = self.eval(&mut state, target);
        }
        state.havoc_everything(&mut self.world);
        self.unplaced(None, true, true);

        // The checker records the header origin first, before appending any
        // origins from later rebindings. Its alias list is SHARE-2's pairwise
        // type-unification result; do not reconstruct it from type equality.
        let possible: Vec<Vec<bool>> = targets
            .iter()
            .map(|target| {
                let origin = self
                    .function
                    .reference_origins
                    .get(target.binding.0 as usize)
                    .and_then(|origins| origins.first());
                targets
                    .iter()
                    .map(|other| {
                        target.binding == other.binding
                            || origin.is_some_and(|origin| {
                                origin.atomic_aliases.contains(&other.binding)
                            })
                    })
                    .collect()
            })
            .collect();
        let objects: Vec<bool> = targets
            .iter()
            .enumerate()
            .map(|(index, target)| {
                matches!(target.kind, CheckedTargetKind::Object)
                    && targets.iter().enumerate().all(|(other, target)| {
                        !possible[index][other]
                            || matches!(target.kind, CheckedTargetKind::Object)
                    })
            })
            .collect();
        let aliases: Vec<Vec<bool>> = possible
            .iter()
            .enumerate()
            .map(|(index, row)| {
                row.iter()
                    .enumerate()
                    .map(|(other, possible)| *possible && objects[index] && objects[other])
                    .collect()
            })
            .collect();
        let any_aliases = possible.iter().enumerate().any(|(index, row)| {
            row.iter()
                .enumerate()
                .any(|(other, possible)| index != other && *possible)
        });
        let mut afters = Vec::new();
        partitions(&aliases, &mut Vec::new(), &mut |representatives| {
            let mut fork = state.clone();
            let mut views: Vec<View> = Vec::new();
            for (index, target) in targets.iter().enumerate() {
                let representative = representatives[index];
                let view = if representative != index {
                    views[representative].clone()
                } else if !objects[index] && any_aliases {
                    // Keyed map targets retain their existing unplaced-write
                    // treatment: map identity alone does not identify an entry.
                    View::Unknown
                } else {
                    View::Place(Location::root(Origin::Constructed(self.world.new_origin())))
                };
                fork.values.insert(target.binding, Value::Ref(view.clone()));
                views.push(view);
            }
            self.enter_atomic(&mut fork, node);
            if let Some(guard) = guard {
                let condition = match self.eval(&mut fork, guard) {
                    Value::Bool(condition, _) => condition,
                    _ => Cond::Unknown,
                };
                if condition.excludes(true) {
                    self.atomic = None;
                    return;
                }
                condition.literals(true, &mut fork.conds);
            }
            let after = self.block(fork, body);
            if let Some(after) = &after {
                self.owe_atomic(after, node, true);
            }
            self.atomic = None;
            if continues
                && let Some(mut after) = after
            {
                after.havoc_everything(&mut self.world);
                afters.push(after);
            }
        });
        join(&mut self.world, state.conds.len(), afters)
    }
}

#[cfg(test)]
mod tests {
    use super::partitions;

    #[test]
    fn three_pairwise_aliases_have_five_identity_cases() {
        let mut cases = Vec::new();
        partitions(&vec![vec![true; 3]; 3], &mut Vec::new(), &mut |case| {
            cases.push(case.to_vec());
        });
        assert_eq!(
            cases,
            [[0, 0, 0], [0, 0, 2], [0, 1, 0], [0, 1, 1], [0, 1, 2]]
        );
    }

    #[test]
    fn possible_aliases_are_not_a_transitive_equivalence_class() {
        let aliases = vec![
            vec![true, true, false],
            vec![true, true, true],
            vec![false, true, true],
        ];
        let mut cases = Vec::new();
        partitions(&aliases, &mut Vec::new(), &mut |case| {
            cases.push(case.to_vec());
        });
        assert_eq!(cases, [[0, 0, 2], [0, 1, 1], [0, 1, 2]]);
    }
}
