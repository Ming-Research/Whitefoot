//! The obligation contract between the structural checker and the entailment
//! engine (`design/compiler/acceptance-records.md`).
//!
//! The checker forms one [`ObligationRecord`] for every mandatory obligation
//! of a function it admitted: each partial operation, call requirement, loop
//! invariant, local invariant, postcondition and submitted separation, with
//! the rule an undischarged record is reported under and the node it is
//! judged at. The engine answers each record it judges with one
//! [`RecordAnswer`], and a function is accepted exactly when every record is
//! answered and discharged. A record the engine leaves unanswered therefore
//! rejects, so an obligation that no walk judges is never accepted, and a
//! family is mapped to its rule once, where the record is formed.

use super::SemanticRule;
use super::entailment::{FunctionEntailment, ObligationFamily, PostconditionDisposition};
use super::model::FunctionId;
use crate::NodePath;

/// One mandatory obligation of a checked function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ObligationRecord {
    /// The rule an undischarged record is reported under, and the rank
    /// [DIAG-1] breaks a tie at one position with.
    pub(crate) rule: SemanticRule,
    /// The node the obligation is judged at: a subscript's `psuffix`, an
    /// operation's carrier, a call, a separation or reference-use site, an
    /// invariant, or an ensures selector.
    pub(crate) site: NodePath,
    pub(crate) subject: ObligationSubject,
}

/// What one record asks, and the identity its answer is found by.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ObligationSubject {
    /// One [ENT-6] obligation of a fixed family at the site: an [OP-4]
    /// subscript, an [OP-2] exact integer operation, an [OP-6] exact
    /// conversion, an [OP-9] allocation, one of a [REF-4] formation's two
    /// conjuncts, or a submitted separation [EFF-5, OP-11, REF-2].
    Source {
        family: ObligationFamily,
        conjunct: u8,
    },
    /// One instantiated callee requirement at the call [FN-8, OP-14].
    CallRequirement {
        callee: FunctionId,
        requires_clause: NodePath,
    },
    /// One source-written loop invariant's induction [INV-1].
    LoopInvariant,
    /// One local invariant, proved by AUTO or by its written certificate
    /// [INV-1, PRF-1].
    SourceProof,
    /// One ensures relation, proved at every selected normal exit [FN-9].
    Postcondition { relation_ordinal: u32 },
}

/// The judgment that answered one record, by its position in the engine's
/// outcome list of that kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordAnswer {
    Obligation(usize),
    CallGoal(usize),
    LoopInvariant(usize),
    SourceProof(usize),
    Postcondition(usize),
    /// [FN-9] the independently established requirements contradict at
    /// body entry, so no exit is reachable and every relation holds.
    Uninhabited,
}

impl RecordAnswer {
    /// Whether the judgment discharged the record. An answer naming no
    /// outcome discharges nothing.
    pub(crate) fn discharged(self, entailment: &FunctionEntailment) -> bool {
        match self {
            Self::Obligation(index) => entailment
                .obligations
                .get(index)
                .is_some_and(|outcome| outcome.discharged),
            Self::CallGoal(index) => entailment.call_goals.get(index).is_some_and(|outcome| {
                outcome.disposition == super::entailment::CallGoalDisposition::Discharged
            }),
            Self::LoopInvariant(index) => entailment
                .loop_invariants
                .get(index)
                .is_some_and(|outcome| outcome.proof.discharged()),
            Self::SourceProof(index) => entailment
                .source_proofs
                .get(index)
                .is_some_and(|outcome| outcome.check.discharged()),
            Self::Postcondition(index) => {
                entailment.postconditions.get(index).is_some_and(|proof| {
                    !proof.exits.is_empty()
                        && proof
                            .exits
                            .iter()
                            .all(|exit| exit.disposition == PostconditionDisposition::Discharged)
                })
            }
            Self::Uninhabited => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::entailment::{
        DerivationId, FunctionEntailment, FunctionPostconditionProof, ObligationFamily,
        ObligationOutcome, PostconditionAggregate, answer_records,
    };
    use super::super::model::{CheckedBodyDisposition, CheckedFunction, CheckedMode, CheckedType};
    use super::{FunctionId, ObligationRecord, ObligationSubject, RecordAnswer};
    use crate::{NodePath, SemanticRule};

    fn path(components: &[u32]) -> NodePath {
        NodePath {
            components: components.to_vec(),
        }
    }

    fn function(obligations: Vec<ObligationRecord>, body: bool) -> CheckedFunction {
        CheckedFunction {
            formal_hypothesis: false,
            id: FunctionId(0),
            declaration: crate::DeclarationId::from_index(0).unwrap(),
            module: crate::ModuleId::BUNDLE_ROOT,
            name: String::new(),
            symbol: String::new(),
            function_actuals: Vec::new(),
            region_parameters: Vec::new(),
            parameters: Vec::new(),
            result_mode: CheckedMode::Own,
            result: CheckedType::Unit,
            declared_state_writes: Vec::new(),
            requirements: Vec::new(),
            requirement_places: Vec::new(),
            postconditions: Vec::new(),
            body: body.then(Vec::new),
            reference_origins: Vec::new(),
            body_disposition: CheckedBodyDisposition::Inhabited,
            allocates: false,
            call_separations: Vec::new(),
            permission_separation_queries: Vec::new(),
            obligations,
            entailment: FunctionEntailment::default(),
        }
    }

    fn subscript(site: &[u32]) -> ObligationRecord {
        ObligationRecord {
            rule: SemanticRule::Op4,
            site: path(site),
            subject: ObligationSubject::Source {
                family: ObligationFamily::Bounds,
                conjunct: 0,
            },
        }
    }

    fn relation(selector: &[u32]) -> ObligationRecord {
        ObligationRecord {
            rule: SemanticRule::Fn9,
            site: path(selector),
            subject: ObligationSubject::Postcondition {
                relation_ordinal: 0,
            },
        }
    }

    fn bounds(site: &[u32], discharged: bool) -> ObligationOutcome {
        ObligationOutcome {
            node_path: path(site),
            family: ObligationFamily::Bounds,
            conjunct: 0,
            canonical_goal: None,
            components: Vec::new(),
            discharged,
            refuted: false,
            contradictory: false,
            residual: (!discharged).then(|| "i < len_of(v)".to_owned()),
            overlap_targets: None,
            derivation: None,
            allocation_length_upper_bound: None,
            allocation_length_upper_bound_derivation: None,
            affine_index_maps: Vec::new(),
            range_partitions: Vec::new(),
            written_before: Vec::new(),
        }
    }

    fn proof(selector: &[u32]) -> FunctionPostconditionProof {
        FunctionPostconditionProof {
            block: path(&[0]),
            selector: path(selector),
            relation_ordinal: 0,
            summary: None,
            exits: Vec::new(),
            aggregate: PostconditionAggregate {
                discharged: false,
                derivation: None,
            },
        }
    }

    #[test]
    fn each_judgment_answers_the_record_formed_at_its_site() {
        let function = function(vec![subscript(&[1]), subscript(&[2])], true);
        let entailment = FunctionEntailment {
            obligations: vec![bounds(&[2], false), bounds(&[1], true)],
            ..FunctionEntailment::default()
        };
        let (answers, unrecorded) = answer_records(&function, &entailment);
        assert_eq!(
            answers,
            [
                Some(RecordAnswer::Obligation(1)),
                Some(RecordAnswer::Obligation(0))
            ]
        );
        assert!(unrecorded.is_empty());
        assert!(RecordAnswer::Obligation(1).discharged(&entailment));
        assert!(!RecordAnswer::Obligation(0).discharged(&entailment));
    }

    /// A record the walk never judged stays unanswered, and a judgment no
    /// record asked for is returned by its site: acceptance refuses both.
    #[test]
    fn a_record_and_a_judgment_that_do_not_meet_are_both_reported() {
        let function = function(vec![subscript(&[1])], true);
        let entailment = FunctionEntailment {
            obligations: vec![bounds(&[3], true)],
            ..FunctionEntailment::default()
        };
        let (answers, unrecorded) = answer_records(&function, &entailment);
        assert_eq!(answers, [None]);
        assert_eq!(unrecorded, [path(&[3])]);
    }

    /// Records and judgments of one identity pair in the order they were
    /// made: two records take the first two judgments, a third judgment is
    /// reported rather than dropped, and a record left without one stays
    /// unanswered.
    #[test]
    fn repeated_judgments_answer_repeated_records_in_order() {
        let function = function(vec![subscript(&[1]), subscript(&[1])], true);
        let entailment = FunctionEntailment {
            obligations: vec![bounds(&[1], false), bounds(&[1], true), bounds(&[1], true)],
            ..FunctionEntailment::default()
        };
        let (answers, unrecorded) = answer_records(&function, &entailment);
        assert_eq!(
            answers,
            [
                Some(RecordAnswer::Obligation(0)),
                Some(RecordAnswer::Obligation(1))
            ]
        );
        assert_eq!(unrecorded, [path(&[1])]);
        let fewer = FunctionEntailment {
            obligations: vec![bounds(&[1], true)],
            ..FunctionEntailment::default()
        };
        assert_eq!(
            answer_records(&function, &fewer).0,
            [Some(RecordAnswer::Obligation(0)), None]
        );
    }

    /// [FN-9] a relation of a body whose requirements contradict holds with
    /// no exit judged; a relation proved nowhere does not.
    #[test]
    fn a_relation_holds_where_the_body_is_uninhabited() {
        let function = function(vec![relation(&[5])], true);
        let uninhabited = FunctionEntailment {
            body_disposition: CheckedBodyDisposition::Uninhabited {
                contradiction: DerivationId(0),
            },
            ..FunctionEntailment::default()
        };
        let (answers, unrecorded) = answer_records(&function, &uninhabited);
        assert_eq!(answers, [Some(RecordAnswer::Uninhabited)]);
        assert!(unrecorded.is_empty());
        let inhabited = FunctionEntailment::default();
        assert_eq!(answer_records(&function, &inhabited).0, [None]);
    }

    /// [PRE-1] a signature without a body publishes its relations as
    /// declaration premises; no record asks for them and none is unrecorded.
    #[test]
    fn a_signature_relation_is_no_obligation() {
        let signature = function(Vec::new(), false);
        let entailment = FunctionEntailment {
            postconditions: vec![proof(&[5])],
            ..FunctionEntailment::default()
        };
        assert!(answer_records(&signature, &entailment).1.is_empty());
        let body = function(Vec::new(), true);
        assert_eq!(answer_records(&body, &entailment).1, [path(&[5])]);
    }
}
