//! The loop summary: the kills a loop body may apply on a path back to
//! its head, formed by the same event formation as the walk, and their
//! application at the head.

use super::*;

impl Input<'_, '_> {
    /// Collects exactly the kill events whose carrying edge can reach this
    /// loop's next head. The return value is the same structural entry
    /// reachability computed by [`Self::loop_block_reaches`].
    pub(super) fn collect_continuing_loop_kills(
        &self,
        statements: &[CheckedStatement],
        normal_reaches: bool,
        reachability: &mut LoopReachability,
        kills: &mut LoopKills,
    ) -> bool {
        let mut reaches = normal_reaches;
        for statement in statements.iter().rev() {
            reaches =
                self.collect_continuing_statement_kills(statement, reaches, reachability, kills);
        }
        reaches
    }

    pub(super) fn collect_continuing_statement_kills(
        &self,
        statement: &CheckedStatement,
        normal_reaches: bool,
        reachability: &mut LoopReachability,
        kills: &mut LoopKills,
    ) -> bool {
        match statement {
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. }
            | CheckedStatement::PropagateLet {
                scrutinee: value, ..
            } => {
                if normal_reaches {
                    self.collect_loop_expression_kills(value, kills);
                }
                normal_reaches
            }
            CheckedStatement::Set {
                node_path,
                target,
                value,
                ..
            } => {
                if normal_reaches {
                    self.collect_set_kills(node_path, target, value, kills);
                }
                normal_reaches
            }
            CheckedStatement::Return { .. } => false,
            CheckedStatement::Give { value, .. } => {
                let reaches = reachability.gives.last().copied().unwrap_or(false);
                if reaches {
                    self.collect_loop_expression_kills(value, kills);
                }
                reaches
            }
            CheckedStatement::Break { target, .. } => reachability.break_reaches(*target),
            CheckedStatement::Proof(_) => normal_reaches,
            CheckedStatement::Match {
                scrutinee, arms, ..
            } => {
                let mut reaches = false;
                for arm in arms {
                    reaches |= self.collect_continuing_loop_kills(
                        &arm.body,
                        normal_reaches,
                        reachability,
                        kills,
                    );
                }
                if reaches {
                    self.collect_loop_expression_kills(scrutinee, kills);
                }
                reaches
            }
            CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => {
                reachability.gives.push(normal_reaches);
                let mut reaches = false;
                for arm in arms {
                    reaches |=
                        self.collect_continuing_loop_kills(&arm.body, false, reachability, kills);
                }
                reachability.gives.pop();
                if reaches {
                    self.collect_loop_expression_kills(scrutinee, kills);
                }
                reaches
            }
            CheckedStatement::Loop { id, body, .. } => {
                reachability.breaks.push((*id, normal_reaches));
                let body_reaches = loop_block_reaches(body, false, reachability);
                self.collect_continuing_loop_kills(body, body_reaches, reachability, kills);
                reachability.breaks.pop();
                normal_reaches || body_reaches
            }
            CheckedStatement::CountedRange {
                id,
                lower,
                upper,
                body,
                ..
            } => {
                reachability.breaks.push((*id, normal_reaches));
                let body_reaches =
                    self.collect_continuing_loop_kills(body, normal_reaches, reachability, kills);
                reachability.breaks.pop();
                // Both endpoint atoms execute before either the real false
                // edge or a body path. Their own effects are continuing for
                // the enclosing target exactly when this statement can reach
                // that target through one of those successors.
                let reaches = normal_reaches || body_reaches;
                if reaches {
                    let mut events = Vec::new();
                    self.collect_expression_kills(lower, &mut events);
                    self.collect_expression_kills(upper, &mut events);
                    kills.push_event_group(events);
                }
                reaches
            }
        }
    }

    pub(super) fn collect_loop_expression_kills(
        &self,
        expression: &CheckedExpression,
        kills: &mut LoopKills,
    ) {
        let mut events = Vec::new();
        self.collect_expression_kills(expression, &mut events);
        kills.push_event_group(events);
    }

    pub(super) fn collect_set_kills(
        &self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        value: &CheckedExpression,
        kills: &mut LoopKills,
    ) {
        let mut events = Vec::new();
        self.collect_expression_kills(value, &mut events);
        self.push_commit_kill(node_path, target, &mut events, kills);
    }

    pub(super) fn push_commit_kill(
        &self,
        node_path: &crate::NodePath,
        target: &CheckedSetTarget,
        events: &mut Vec<KillEvent>,
        kills: &mut LoopKills,
    ) {
        kills.set_bindings.insert(target.binding());
        events.push(self.commit_kill(node_path, target));
        kills.push_event_group(std::mem::take(events));
    }
}

impl Reasoning<'_, '_, '_> {
    pub(super) fn apply_loop_kills_one(
        &mut self,
        separations: &dyn SeparationOracle,
        state: &mut FactState,
        kills: &LoopKills,
    ) {
        self.vocabulary
            .materialize_before_event_kill(state, &kills.events);
        state.kill(|term| {
            kills
                .events
                .iter()
                .any(|event| self.event_kills_term(separations, term, event))
        });
        for event in &kills.events {
            self.kill_s12_candidates_for_event(separations, state, event);
        }
        state.kill_goals(|goal| {
            kills
                .events
                .iter()
                .any(|event| self.event_kills_goal(separations, goal, event))
        });
        state.goal_origins.retain(|binding, _| {
            !kills.events.iter().any(|event| {
                self.input
                    .event_kills_goal_origin_binding(separations, *binding, event)
            })
        });
        state
            .origins
            .retain(|binding, _| !kills.set_bindings.contains(binding));
        state
            .goal_origins
            .retain(|binding, _| !kills.set_bindings.contains(binding));
        state
            .ambiguous_goal_origins
            .retain(|binding| !kills.set_bindings.contains(binding));
    }

    pub(super) fn apply_loop_kills(
        &mut self,
        states: &mut ProofFlowState,
        kills: &LoopKills,
        event: Option<FlowEventId>,
    ) {
        self.vocabulary.promote_flow_contradiction(states);
        // The header kills stand for the body's events on every iteration,
        // and an index this state proves live stays live at each of them
        // [WIN-2], as a range this state proves to end at or below `r.len`
        // stays within it: `r.len` falls only at an event that writes
        // `r.last`, `r.filled` or the whole window [OP-10], each of which
        // kills every fact below `r[i]` or `r[lo..hi]` here because [WIN-2]
        // fixes every index and range as overlapping `r.last` and `r.filled`,
        // a write of `i` kills the fact through its offset support [ENT-5],
        // and a range's endpoints are captured values no write changes
        // [OWN-7]. A fact these kills leave therefore meets no such event.
        let ledger = states.separations.clone();
        let live = self.event_live_bounds(states, &kills.events);
        let separations = EventSeparations {
            ledger: &ledger,
            live: &live,
        };
        self.kill_result_evidence(states, &kills.events);
        self.apply_loop_kills_one(&separations, &mut states.facts, kills);
        self.apply_affine_kills(&separations, &mut states.affine, &kills.events);
        states.record_writes(&kills.events);
        let mut groups = kills.entry_image_groups.iter().collect::<Vec<_>>();
        groups.sort_by(|left, right| left.owner.components().cmp(right.owner.components()));
        for group in groups {
            self.invalidate_entry_images(
                states,
                &separations,
                &kills.events[group.range.clone()],
                event,
            );
        }
    }
}

/// [ENT-5] a path reaching a loop's back edge applied only events the
/// loop's summary subtracted at its head; an event the summary misses
/// would leave a stale fact at the head.
pub(super) fn debug_assert_summarized(state: &ProofFlowState, kills: &LoopKills) {
    debug_assert!(
        state
            .continuing
            .iter()
            .all(|event| kills.events.contains(event)),
        "[ENT-5] a continuing kill is missing from its loop summary: {:?}",
        state
            .continuing
            .iter()
            .find(|event| !kills.events.contains(event))
    );
}

/// Returns whether a block entry can reach the loop head whose summary is
/// being built. `normal_reaches` describes the containing block's normal
/// exit. This is structural reachability over [FN-1], not an executable
/// constant-folding judgment.
pub(super) fn loop_block_reaches(
    statements: &[CheckedStatement],
    normal_reaches: bool,
    reachability: &mut LoopReachability,
) -> bool {
    let mut reaches = normal_reaches;
    for statement in statements.iter().rev() {
        reaches = loop_statement_reaches(statement, reaches, reachability);
    }
    reaches
}

pub(super) fn loop_statement_reaches(
    statement: &CheckedStatement,
    normal_reaches: bool,
    reachability: &mut LoopReachability,
) -> bool {
    match statement {
        CheckedStatement::Let { .. }
        | CheckedStatement::DestructuringLet { .. }
        | CheckedStatement::PropagateLet { .. }
        | CheckedStatement::Set { .. }
        | CheckedStatement::Evaluate { .. }
        | CheckedStatement::DropExpression { .. }
        | CheckedStatement::Proof(_) => normal_reaches,
        CheckedStatement::Return { .. } => false,
        CheckedStatement::Give { .. } => reachability.gives.last().copied().unwrap_or(false),
        CheckedStatement::Break { target, .. } => reachability.break_reaches(*target),
        CheckedStatement::Match { arms, .. } => {
            let mut reaches = false;
            for arm in arms {
                reaches |= loop_block_reaches(&arm.body, normal_reaches, reachability);
            }
            reaches
        }
        CheckedStatement::ValueMatchLet { arms, .. } => {
            // Arm fallthrough never reaches a value initializer's
            // continuation. Its `give` edges do, and nested value
            // initializers shadow this target while they are inspected.
            reachability.gives.push(normal_reaches);
            let mut reaches = false;
            for arm in arms {
                reaches |= loop_block_reaches(&arm.body, false, reachability);
            }
            reachability.gives.pop();
            reaches
        }
        CheckedStatement::Loop { id, body, .. } => {
            // A nested loop body reaches its successor through its own
            // break edges, or can escape through another visible target.
            // A backedge alone cannot create reachability, so evaluating
            // the body with a false normal exit computes the least fixed
            // point. Once the body entry reaches the target, its normal
            // exit can take another iteration and eventually use that
            // same route.
            reachability.breaks.push((*id, normal_reaches));
            let body_reaches = loop_block_reaches(body, false, reachability);
            reachability.breaks.pop();
            // [FN-1] also keeps a conservative direct edge from the
            // nested loop statement to its normal successor. That edge
            // carries no event from inside the body.
            normal_reaches || body_reaches
        }
        CheckedStatement::CountedRange { id, body, .. } => {
            // The false-header edge reaches the normal successor, while
            // body fallthrough updates and returns to a header that may
            // then take that same edge. A matching break also reaches the
            // successor; enclosing exits retain their visible targets.
            reachability.breaks.push((*id, normal_reaches));
            let body_reaches = loop_block_reaches(body, normal_reaches, reachability);
            reachability.breaks.pop();
            normal_reaches || body_reaches
        }
    }
}
