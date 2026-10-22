//! Exact value evaluation and operation relations in the range walk
//! [RANGE-2, ENT-3.S7]. The ordinary and range judgments share the S7 table.

use super::*;
use crate::semantic::entailment::operation_facts::{self, Row, Span};

impl Walker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn integer_operation(
        &mut self,
        state: &mut State,
        carrier: &NodePath,
        operation: CheckedIntegerOperation,
        operand_type: CheckedType,
        arguments: &[CheckedExpression],
        result: CheckedType,
    ) -> Value {
        let values = arguments
            .iter()
            .map(|argument| self.int(state, argument))
            .collect::<Vec<_>>();
        let mut comparison = |relation| {
            let condition = match values.as_slice() {
                [left, right] => Cond::Literal(literal(left.clone(), relation, right.clone())),
                _ => Cond::Unknown,
            };
            self.world.boolean(condition)
        };
        match operation {
            CheckedIntegerOperation::Equal => return comparison(Relation::Equal),
            CheckedIntegerOperation::NotEqual => return comparison(Relation::NotEqual),
            CheckedIntegerOperation::Less => return comparison(Relation::Less),
            CheckedIntegerOperation::LessEqual => return comparison(Relation::LessEqual),
            CheckedIntegerOperation::Greater => return comparison(Relation::Greater),
            CheckedIntegerOperation::GreaterEqual => return comparison(Relation::GreaterEqual),
            CheckedIntegerOperation::AddExact
            | CheckedIntegerOperation::SubtractExact
            | CheckedIntegerOperation::MultiplyExact => {
                // An absent outer option is nonlinear; an absent inner one
                // is a failure of this checker's finite arithmetic.
                let formed = match (operation, values.as_slice()) {
                    (CheckedIntegerOperation::AddExact, [left, right]) => Some(left.plus(right)),
                    (CheckedIntegerOperation::SubtractExact, [left, right]) => {
                        Some(left.minus(right))
                    }
                    (CheckedIntegerOperation::MultiplyExact, [left, right])
                        if left.is_constant() =>
                    {
                        Some(right.scaled(left.constant))
                    }
                    (CheckedIntegerOperation::MultiplyExact, [left, right])
                        if right.is_constant() =>
                    {
                        Some(left.scaled(right.constant))
                    }
                    _ => None,
                };
                let arithmetic_limit = matches!(formed, Some(None));
                let exact = formed.flatten();
                let goal = exact
                    .as_ref()
                    .map_or(Value::Unknown, |value| Value::Int(value.clone()));
                if arithmetic_limit {
                    // The result is forgotten whether or not this site owes
                    // an ordinary goal; later obligations may need it.
                    self.arithmetic.get_or_insert_with(|| carrier.clone());
                    self.ordinary_arithmetic_limit(
                        carrier,
                        &ObligationSubject::Source {
                            family: ObligationFamily::IntegerDomain,
                            conjunct: 0,
                        },
                    );
                    return self.opaque_of(result);
                }
                self.ordinary_domain(
                    state,
                    carrier,
                    ObligationFamily::IntegerDomain,
                    &goal,
                    result,
                );
                if let Some(exact) = exact {
                    return Value::Int(exact);
                }
            }
            CheckedIntegerOperation::DivideExact
            | CheckedIntegerOperation::RemainderExact
            | CheckedIntegerOperation::NegateExact
            | CheckedIntegerOperation::AbsoluteExact
            | CheckedIntegerOperation::ShiftLeftExact
            | CheckedIntegerOperation::ShiftRightExact => {
                let goal = integer_type(result)
                    .and_then(|ty| ordinary::integer_domain(operation, ty, &values));
                self.ordinary_counterexamples(
                    state,
                    carrier,
                    &ObligationSubject::Source {
                        family: ObligationFamily::IntegerDomain,
                        conjunct: 0,
                    },
                    goal,
                    Some(&[]),
                );
            }
            _ => {}
        }
        match (
            Row::of(operation),
            integer_type(operand_type),
            integer_type(result),
        ) {
            (Some(row), Some(operand), Some(result)) => {
                self.operation_value(state, carrier, row, operand, result, &values)
            }
            _ => self.opaque_of(result),
        }
    }

    /// A row outside exact range evaluation yields fresh r and precisely
    /// its operand relations [RANGE-2]. Result bounds belong to ordinary S7;
    /// the range walk adds only the result's own type bounds [RANGE-3].
    #[allow(clippy::too_many_arguments)]
    pub(super) fn operation_value(
        &mut self,
        state: &mut State,
        carrier: &NodePath,
        row: Row,
        operand: IntegerType,
        result: IntegerType,
        values: &[Linear],
    ) -> Value {
        let value = self.world.opaque(Some(result));
        let intervals = match self.function.entailment.operation_intervals.get(carrier) {
            Some(Some(intervals)) => intervals,
            Some(None) => return Value::Int(value),
            None => {
                if self.dry == 0 {
                    self.issues.push(RangeIssue::Unsupported {
                        node: carrier.clone(),
                        feature: UnsupportedSemanticFeature::RangeOperationIntervals,
                    });
                }
                return Value::Int(value);
            }
        };
        debug_assert_eq!(intervals.len(), values.len());
        let facts = operation_facts::facts(
            row,
            Span::of(operand),
            Span::of(result),
            intervals,
            None,
        );
        for relation in facts.relations {
            let operand = values
                .get(relation.operand)
                .expect("an S7 relation names one operand of its checked operation");
            if let Some(low) = relation.low {
                let Some(bound) = operand.plus_constant(low) else {
                    if self.dry == 0 {
                        self.issues.push(RangeIssue::Unsupported {
                            node: carrier.clone(),
                            feature: UnsupportedSemanticFeature::RangeArithmetic,
                        });
                    }
                    return Value::Int(value);
                };
                state.conds.push(literal(
                    value.clone(),
                    Relation::GreaterEqual,
                    bound,
                ));
            }
            if let Some(high) = relation.high {
                let Some(bound) = operand.plus_constant(high) else {
                    if self.dry == 0 {
                        self.issues.push(RangeIssue::Unsupported {
                            node: carrier.clone(),
                            feature: UnsupportedSemanticFeature::RangeArithmetic,
                        });
                    }
                    return Value::Int(value);
                };
                state.conds.push(literal(
                    value.clone(),
                    Relation::LessEqual,
                    bound,
                ));
            }
        }
        Value::Int(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::entailment::operation_facts::Interval;
    use crate::semantic::tests::with_semantics;

    const SOURCE: &[u8] = br#"fn inspect(items: &Slots<u64>, n: u64) -> result: unit pure contract {
  requires n <= 7_u64;
  requires forall marker(k in 0_u64..items^.len): 0_u64 <= items^[k];
} {
  set n = n +wrap 1_u64;
  let minimum = imin(n, 7_u64);
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  return std::process::exit_status(code: 0_u8);
}
"#;

    /// [ENT-3.S7, RANGE-2] interval capture precedes SET-1's target kill;
    /// relations preserve the operand's evaluated storage version, include
    /// literals, and repeated evaluations produce distinct unknown results.
    #[test]
    fn captured_intervals_and_operand_relations_keep_the_statement_value() {
        with_semantics(SOURCE, |outcome| {
            let crate::SemanticOutcome::Complete(program) = outcome else {
                panic!("the unused operation source must check: {outcome:?}");
            };
            let data = &program.data;
            let functions = data.functions.iter().collect::<Vec<_>>();
            let function = data
                .executable_functions()
                .find(|function| function.name == "inspect")
                .unwrap();
            let body = function.body.as_ref().unwrap();
            let CheckedStatement::Set {
                value: CheckedExpression::IntegerOperation { carrier: commit, .. },
                ..
            } = &body[0]
            else {
                panic!("first statement must be the wrapping commit");
            };
            let CheckedStatement::Let {
                value: CheckedExpression::IntegerOperation { carrier: minimum, .. },
                ..
            } = &body[1]
            else {
                panic!("second statement must be imin");
            };
            assert_eq!(
                function.entailment.operation_intervals[commit],
                Some(vec![Interval::new(0, 7), Interval::value(1)]),
                "the commit must read n before overwriting it"
            );
            assert_eq!(
                function.entailment.operation_intervals[minimum],
                Some(vec![Interval::new(1, 8), Interval::value(7)]),
                "the next statement must read the committed n"
            );

            let prelude = BTreeSet::new();
            let mut walker = Walker::new(
                &functions,
                &data.nominals,
                &data.elements,
                function,
                Vec::new(),
                &data.constants,
                JudgmentScope::Concrete,
                &prelude,
            );
            let mut state = State::default();
            let container = state
                .container(
                    &mut walker.world,
                    Location::root(Origin::Parameter(function.parameters[0].binding)),
                    1,
                )
                .unwrap();
            let version = state.version(&mut walker.world, container);
            let read = walker.world.read(
                version,
                vec![Linear::constant(0)],
                Vec::new(),
                Some(IntegerType::U64),
            );
            let operands = [read.clone(), Linear::constant(7)];
            let Value::Int(first) = walker.operation_value(
                &mut state,
                minimum,
                Row::Minimum,
                IntegerType::U64,
                IntegerType::U64,
                &operands,
            )
            else {
                panic!("imin must yield an integer");
            };
            assert_eq!(
                state.conds,
                vec![
                    literal(first.clone(), Relation::LessEqual, read.clone()),
                    literal(first.clone(), Relation::LessEqual, Linear::constant(7)),
                ],
                "only operand relations, including the literal's, join the range path"
            );
            state.havoc_container(&mut walker.world, container, false);
            let version = state.version(&mut walker.world, container);
            let changed = walker.world.read(
                version,
                vec![Linear::constant(0)],
                Vec::new(),
                Some(IntegerType::U64),
            );
            assert_ne!(read, changed);
            assert!(!state.conds.contains(&literal(
                first.clone(),
                Relation::LessEqual,
                changed.clone(),
            )));
            let Value::Int(second) = walker.operation_value(
                &mut state,
                minimum,
                Row::Minimum,
                IntegerType::U64,
                IntegerType::U64,
                &[changed, Linear::constant(7)],
            )
            else {
                panic!("the second imin must yield an integer");
            };
            assert_ne!(second, first);
            assert!(walker.issues.is_empty());

            // A synthetic operand value reaches the representation boundary
            // while the row's captured intervals remain representable. A dry
            // write-discovery pass cannot publish that proof limitation.
            for dry in [1, 0] {
                walker.dry = dry;
                let mut state = State::default();
                let _ = walker.operation_value(
                    &mut state,
                    minimum,
                    Row::Add { wrap: true },
                    IntegerType::U64,
                    IntegerType::U64,
                    &[Linear::constant(i128::MAX), Linear::constant(7)],
                );
                assert!(state.conds.is_empty());
                assert_eq!(
                    walker.issues,
                    if dry == 0 {
                        vec![RangeIssue::Unsupported {
                            node: minimum.clone(),
                            feature: UnsupportedSemanticFeature::RangeArithmetic,
                        }]
                    } else {
                        Vec::new()
                    }
                );
            }
        });
    }

    /// [ENT-3.S7, RANGE-2] a contradictory snapshot supplies no operation
    /// fact, whereas a missing live snapshot is a compiler capability gap.
    /// Dry walks learn writes only and cannot report missing proof context.
    #[test]
    fn contradictory_and_missing_operation_contexts_are_distinct() {
        with_semantics(SOURCE, |outcome| {
            let crate::SemanticOutcome::Complete(program) = outcome else {
                panic!("the unused operation source must check: {outcome:?}");
            };
            let data = &program.data;
            let functions = data.functions.iter().collect::<Vec<_>>();
            let original = data
                .executable_functions()
                .find(|function| function.name == "inspect")
                .unwrap();
            let CheckedStatement::Let {
                value: CheckedExpression::IntegerOperation { carrier, .. },
                ..
            } = &original.body.as_ref().unwrap()[1]
            else {
                panic!("second statement must be imin");
            };
            let prelude = BTreeSet::new();
            for (context, missing) in [(Some(None), false), (None, true)] {
                let mut function = original.clone();
                function.entailment.operation_intervals.remove(carrier);
                if let Some(context) = context {
                    function
                        .entailment
                        .operation_intervals
                        .insert(carrier.clone(), context);
                }
                let mut walker = Walker::new(
                    &functions,
                    &data.nominals,
                    &data.elements,
                    &function,
                    Vec::new(),
                    &data.constants,
                    JudgmentScope::Concrete,
                    &prelude,
                );
                let mut state = State::default();
                let operands = [Linear::constant(1), Linear::constant(7)];
                let _ = walker.operation_value(
                    &mut state,
                    carrier,
                    Row::Minimum,
                    IntegerType::U64,
                    IntegerType::U64,
                    &operands,
                );
                assert!(state.conds.is_empty());
                assert_eq!(
                    walker.issues,
                    if missing {
                        vec![RangeIssue::Unsupported {
                            node: carrier.clone(),
                            feature: UnsupportedSemanticFeature::RangeOperationIntervals,
                        }]
                    } else {
                        Vec::new()
                    }
                );
                walker.issues.clear();
                walker.dry = 1;
                let _ = walker.operation_value(
                    &mut state,
                    carrier,
                    Row::Minimum,
                    IntegerType::U64,
                    IntegerType::U64,
                    &operands,
                );
                assert!(walker.issues.is_empty());
            }
        });
    }
}
