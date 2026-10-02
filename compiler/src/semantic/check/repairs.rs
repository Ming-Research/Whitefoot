//! The words of the repair a goal's rejection carries [DIAG-1, MSR-4].
//!
//! [DIAG-1] fixes what a repair may direct and leaves its words to the
//! toolchain: carried out as it directs, each alternative lets the rejected
//! judgment succeed in a state that is not contradictory, an alternative that
//! needs a condition the checker has not decided states it, and no alternative
//! writes text a rule rejects wherever it stands. For a goal that choice turns
//! on two things the checker knows. A refuted goal is false where it stands
//! [ENT-4], so establishing it there only makes the point contradictory; its
//! repair changes what reaches the construct, or the operation or clause that
//! poses the goal. An unproved goal is established by a fact source its terms
//! admit: a `requires` on the enclosing function when every term is a
//! parameter no event on a path to the goal writes, since that fact then
//! still holds where the goal is asked; a proof whose premises the checker
//! cannot guess otherwise, which is why those routes state the condition they
//! need, the callee's `ensures` among them only when the goal reads a value a
//! call returned; or a guard where skipping the construct is the program's
//! intent. An operand
//! that is no term [ENT-2], such as an element read, admits none of these
//! until a `let` binds it, so that binding is its repair.
//!
//! The words of [TYPE-2]'s refusals of an opaque struct's construction and
//! destructuring live here too. They turn on where the struct comes from,
//! whether the consumed place owns its value and, for a cell, on its content,
//! since those decide which source change can be carried out.
//!
//! So do the words of the fix a [STOR-6] target-layout stop carries when the
//! selected target cannot hold an allocation's proved count bound. That stop
//! is no rejection [DIAG-1], but it sends the writer to the same count as
//! [OP-9]'s repair, whose words stand next to it.
//!
//! Selector admission and invariant-name scope repairs also live here. The
//! resolver calls the scope formatter with the name it has already classified;
//! that formatter performs no semantic checking.
//!
//! The sentences live here, in one place, so that wording can follow evidence
//! from agents without touching the judgments that select them.

use std::collections::{BTreeSet, HashSet};

use super::super::entailment::TermRead;
use super::super::goal::{
    EvaluatedValueOccurrence, GoalDatum, GoalExpression, GoalOperation, GoalProjection,
};
use super::super::model::{
    BindingId, CheckedCallSeparationPositions, CheckedConversionMode, CheckedExpression,
    CheckedFunction, CheckedIntegerOperation, CheckedStatement, FunctionId, expression_children,
};
use super::super::permission::visit_read_bindings;
use crate::NodePath;

/// [FN-9] an unsupported selector cannot state this postcondition. Removing
/// its last clause also removes a now-empty or define-only contract [FN-8].
pub(crate) const fn postcondition_selector_repair() -> &'static str {
    "remove this ensures clause, and remove its contract block if no requires or ensures clauses remain; an unrouted clause can name only result data admitted by [CALL-4]; a routed clause selects `when b is Ok(value: r):` for an own Result<T, E> or `when b is Some(value: r):` for an own Option<T>, where b names that result, r is fresh and payload T supplies admitted data [FN-9]; omit `b is` only when exactly one declared result has the route's enum type [CALL-4]; Err, None and user-enum variants are not postcondition routes"
}

/// [INV-1, ENT-5] the name's scope and the conclusion's survival are separate.
/// An explicit certificate may use a surviving relation, but not its expired
/// header name; a target AUTO already proves needs no proof block [PRF-1].
pub(crate) fn header_invariant_scope_repair(name: &str) -> String {
    format!(
        "header invariant `{name}` can be named only inside its loop body [INV-1]; its conclusion survives only under the ordinary fact rules [ENT-5]: if AUTO proves this target, remove its proof block; otherwise, if an available relation with in-scope terms supplies the same premise as `{name}`, replace `{name}` in this use with `(relation)`, keeping `use` and any `k times` coefficient [PRF-1]"
    )
}

/// Whether the checker derived a goal false or derived neither sign [ENT-4].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Disposition {
    Refuted,
    Unproved,
}

/// What a goal's terms are, which selects the routes that can establish it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GoalTerms {
    /// Every term is a constant or a parameter of the enclosing function that
    /// no event on a path to the goal writes, at least one of them a
    /// parameter: a `requires` naming the goal holds at entry and still holds
    /// where the goal is asked.
    Parameters,
    /// The goal reads the value one argument has inside the call itself,
    /// which no fact can name [FN-8].
    CallArgument(u32),
    /// The goal reads a value only its own occurrence identifies, or an
    /// element of a range formed where it is used: no condition has it as
    /// its goal origin and no fact names it until a `let` binds it [ENT-2,
    /// ENT-3].
    Unnamed,
    /// The goal reads the `len` of a range an argument formed at the call,
    /// which is the difference of that range's endpoints [REF-4]: the
    /// requirement bounds that difference.
    RangeLength,
    /// The goal reads a computed or local value, or an element, which a
    /// condition naming the same admitted expression establishes [ENT-3].
    Computed,
}

/// What a goal reads, as its repair needs it.
#[derive(Clone, Copy, Debug)]
pub(super) struct GoalReads {
    pub(super) terms: GoalTerms,
    /// Some read goes through a reference parameter [EFF-2].
    pub(super) referenced: bool,
    /// Some read is of a value a user call returned, which that callee's
    /// `ensures` can bound [FN-9].
    pub(super) called: bool,
}

impl GoalTerms {
    /// Classifies a concrete goal over the enclosing function's bindings,
    /// `written` being every binding some event on a path to the goal writes
    /// or consumes, sorted.
    pub(super) fn of_goal(
        goal: &GoalExpression,
        function: &CheckedFunction,
        written: &[BindingId],
        editable: &HashSet<FunctionId>,
    ) -> GoalReads {
        let mut reads = Reads::new(function, written, editable);
        reads.goal(goal);
        reads.finish()
    }

    /// Classifies the terms one obligation's normalized relations read.
    pub(super) fn of_terms(
        terms: &[TermRead],
        function: &CheckedFunction,
        written: &[BindingId],
        editable: &HashSet<FunctionId>,
    ) -> GoalReads {
        let mut reads = Reads::new(function, written, editable);
        for term in terms {
            match term {
                TermRead::Constant => {}
                TermRead::Binding(binding) => reads.binding(*binding),
                TermRead::Unnamed => reads.unnamed = true,
                TermRead::Computed => reads.computed = true,
            }
        }
        reads.finish()
    }
}

struct Reads<'a> {
    function: &'a CheckedFunction,
    written: &'a [BindingId],
    editable: &'a HashSet<FunctionId>,
    parameter: bool,
    computed: bool,
    unnamed: bool,
    /// Some measure read is the `len` of a range formed at the call.
    range_length: bool,
    argument: Option<u32>,
    /// Some read goes through a reference parameter, which executable code
    /// that repeats it must declare in the row [EFF-2].
    referenced: bool,
    /// Every binding a read is rooted at.
    bindings: Vec<BindingId>,
}

impl<'a> Reads<'a> {
    const fn new(
        function: &'a CheckedFunction,
        written: &'a [BindingId],
        editable: &'a HashSet<FunctionId>,
    ) -> Self {
        Self {
            function,
            written,
            editable,
            parameter: false,
            computed: false,
            unnamed: false,
            range_length: false,
            argument: None,
            referenced: false,
            bindings: Vec::new(),
        }
    }

    fn binding(&mut self, binding: BindingId) {
        self.bindings.push(binding);
        let parameter = self
            .function
            .parameters
            .iter()
            .find(|parameter| parameter.binding == binding);
        self.referenced |= parameter.is_some_and(|parameter| parameter.mode.is_reference());
        if parameter.is_some() && self.written.binary_search(&binding).is_err() {
            self.parameter = true;
        } else {
            self.computed = true;
        }
    }

    fn goal(&mut self, goal: &GoalExpression) {
        match goal {
            // An admitted element read is part of the goal's identity, which
            // a guard establishes; no clause names it as a parameter does.
            GoalExpression::Operation {
                row:
                    GoalOperation::ArrayIndex { .. }
                    | GoalOperation::BufferIndex { .. }
                    | GoalOperation::RunIndex { .. },
                arguments,
                ..
            } => {
                self.computed = true;
                for argument in arguments {
                    self.goal(argument);
                }
            }
            // A measure is a term of the place it measures [MSR-1], which a
            // clause spells when every step of that place is spelled.
            GoalExpression::Operation {
                row:
                    GoalOperation::ContainerMeasure { .. }
                    | GoalOperation::ArrayMeasure { .. }
                    | GoalOperation::BufferMeasure { .. },
                arguments,
                ..
            } => {
                for argument in arguments {
                    self.measured(argument);
                }
            }
            GoalExpression::Operation { arguments, .. } => {
                for argument in arguments {
                    self.goal(argument);
                }
            }
            GoalExpression::Datum(datum) => self.datum(datum),
        }
    }

    fn datum(&mut self, datum: &GoalDatum) {
        match datum {
            GoalDatum::Literal(_) => {}
            GoalDatum::NamedConst { projections, .. } if spelled(projections) => {}
            GoalDatum::Place {
                root, projections, ..
            } if spelled(projections) => self.binding(*root),
            GoalDatum::EvaluatedValue {
                occurrence: EvaluatedValueOccurrence::CallArgument { argument, .. },
                ..
            } => {
                self.argument.get_or_insert(*argument);
            }
            // A value only its occurrence identifies has no goal origin
            // [ENT-3], and a range formed at its use is no measure place
            // [ENT-2].
            GoalDatum::EvaluatedValue { .. } => self.unnamed = true,
            GoalDatum::NamedConst { projections, .. } | GoalDatum::Place { projections, .. }
                if ranged(projections) =>
            {
                self.unnamed = true;
            }
            _ => self.computed = true,
        }
    }

    /// The place a measure reads. A measure of an element is a term
    /// [MSR-1], but its offset is captured where the place is formed and no
    /// clause can name that capture, so it counts as computed.
    fn measured(&mut self, place: &GoalExpression) {
        match place {
            GoalExpression::Datum(GoalDatum::NamedConst { projections, .. })
                if spelled(projections) => {}
            GoalExpression::Datum(GoalDatum::Place {
                root, projections, ..
            }) if spelled(projections) => self.binding(*root),
            GoalExpression::Datum(
                datum @ GoalDatum::EvaluatedValue {
                    occurrence: EvaluatedValueOccurrence::CallArgument { .. },
                    ..
                },
            ) => self.datum(datum),
            // [REF-4] the `len` of a range formed at its use is the
            // difference of its endpoints, which the repair names.
            GoalExpression::Datum(GoalDatum::Place { projections, .. }) if ranged(projections) => {
                self.range_length = true;
            }
            _ => self.computed = true,
        }
    }

    fn finish(self) -> GoalReads {
        let terms = match self {
            Self {
                argument: Some(argument),
                ..
            } => GoalTerms::CallArgument(argument),
            Self { unnamed: true, .. } => GoalTerms::Unnamed,
            Self {
                range_length: true, ..
            } => GoalTerms::RangeLength,
            Self {
                parameter: true,
                computed: false,
                ..
            } => GoalTerms::Parameters,
            _ => GoalTerms::Computed,
        };
        let results = call_results(self.function, self.editable);
        GoalReads {
            terms,
            referenced: self.referenced,
            called: self
                .bindings
                .iter()
                .any(|binding| results.contains(binding)),
        }
    }
}

/// The bindings of `function` whose value a call to one of the `editable`
/// functions returned, directly or through local computation from such a
/// value: the values a callee's `ensures` the writer can add can bound
/// [FN-9]. A prelude function's contract is not the writer's to change. The
/// closure ignores control flow, which only widens it.
fn call_results(function: &CheckedFunction, editable: &HashSet<FunctionId>) -> BTreeSet<BindingId> {
    let mut definitions = Vec::new();
    if let Some(body) = &function.body {
        collect_definitions(body, None, editable, &mut definitions);
    }
    let mut results: BTreeSet<BindingId> = definitions
        .iter()
        .filter(|definition| definition.call)
        .map(|definition| definition.binding)
        .collect();
    loop {
        let before = results.len();
        for definition in &definitions {
            if definition.reads.iter().any(|read| results.contains(read)) {
                results.insert(definition.binding);
            }
        }
        if results.len() == before {
            return results;
        }
    }
}

/// One value a statement gives a binding: the bindings it reads, and whether
/// a user call computes it.
struct Definition {
    binding: BindingId,
    reads: Vec<BindingId>,
    call: bool,
}

impl Definition {
    fn of(
        binding: BindingId,
        values: &[&CheckedExpression],
        editable: &HashSet<FunctionId>,
    ) -> Self {
        let mut reads = Vec::new();
        for value in values {
            visit_read_bindings(value, &mut |read| reads.push(read));
        }
        Self {
            binding,
            reads,
            call: values.iter().any(|value| calls(value, editable)),
        }
    }
}

/// Whether an expression tree contains a call to one of the `editable`
/// functions.
fn calls(expression: &CheckedExpression, editable: &HashSet<FunctionId>) -> bool {
    matches!(expression, CheckedExpression::UserCall { function, .. } if editable.contains(function))
        || expression_children(expression)
            .into_iter()
            .any(|child| calls(child, editable))
}

/// Every value the statements give a binding, `give` naming the binding a
/// value initializer's `give` delivers to.
fn collect_definitions(
    statements: &[CheckedStatement],
    give: Option<BindingId>,
    editable: &HashSet<FunctionId>,
    definitions: &mut Vec<Definition>,
) {
    for statement in statements {
        match statement {
            CheckedStatement::Let { binding, value, .. }
            | CheckedStatement::PropagateLet {
                binding,
                scrutinee: value,
                ..
            } => definitions.push(Definition::of(*binding, &[value], editable)),
            CheckedStatement::DestructuringLet {
                bindings, value, ..
            } => {
                for (binding, _, _) in bindings {
                    definitions.push(Definition::of(*binding, &[value], editable));
                }
            }
            CheckedStatement::Set { target, value, .. } => {
                definitions.push(Definition::of(target.binding(), &[value], editable));
            }
            CheckedStatement::Give { value, .. } => {
                if let Some(binding) = give {
                    definitions.push(Definition::of(binding, &[value], editable));
                }
            }
            CheckedStatement::Match {
                scrutinee, arms, ..
            } => {
                for arm in arms {
                    for binder in &arm.binders {
                        definitions.push(Definition::of(binder.binding, &[scrutinee], editable));
                    }
                    collect_definitions(&arm.body, give, editable, definitions);
                }
            }
            CheckedStatement::ValueMatchLet {
                binding,
                scrutinee,
                arms,
                ..
            } => {
                for arm in arms {
                    for binder in &arm.binders {
                        definitions.push(Definition::of(binder.binding, &[scrutinee], editable));
                    }
                    collect_definitions(&arm.body, Some(*binding), editable, definitions);
                }
            }
            CheckedStatement::Loop { body, .. } => {
                collect_definitions(body, give, editable, definitions);
            }
            CheckedStatement::CountedRange {
                binder,
                lower,
                upper,
                body,
                ..
            } => {
                definitions.push(Definition::of(*binder, &[lower, upper], editable));
                collect_definitions(body, give, editable, definitions);
            }
            CheckedStatement::Atomic {
                binding,
                target,
                body,
                ..
            } => {
                definitions.push(Definition::of(*binding, &[target], editable));
                collect_definitions(body, give, editable, definitions);
            }
            CheckedStatement::Evaluate { .. }
            | CheckedStatement::DropExpression { .. }
            | CheckedStatement::Proof(_)
            | CheckedStatement::Return { .. }
            | CheckedStatement::Break { .. } => {}
        }
    }
}

/// Whether the value the `return` at `statement` delivers is, or reads, a
/// value a call to one of the `editable` functions returned, which the
/// callee's `ensures` can bound [FN-9].
pub(super) fn returns_call_result(
    function: &CheckedFunction,
    statement: &NodePath,
    editable: &HashSet<FunctionId>,
) -> bool {
    let results = call_results(function, editable);
    function
        .body
        .as_deref()
        .and_then(|body| returned_value(body, statement))
        .is_some_and(|value| {
            let mut read = false;
            visit_read_bindings(value, &mut |binding| read |= results.contains(&binding));
            read || calls(value, editable)
        })
}

/// The value of the `return` at `path`, wherever it is nested.
fn returned_value<'a>(
    statements: &'a [CheckedStatement],
    path: &NodePath,
) -> Option<&'a CheckedExpression> {
    statements.iter().find_map(|statement| match statement {
        CheckedStatement::Return {
            node_path, value, ..
        } => (node_path == path).then_some(value),
        CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
            arms.iter().find_map(|arm| returned_value(&arm.body, path))
        }
        CheckedStatement::Loop { body, .. }
        | CheckedStatement::CountedRange { body, .. }
        | CheckedStatement::Atomic { body, .. } => returned_value(body, path),
        _ => None,
    })
}

/// Field and Box-content selections are steps a clause can spell as written.
fn spelled(projections: &[GoalProjection]) -> bool {
    projections
        .iter()
        .all(|projection| matches!(projection, GoalProjection::Deref | GoalProjection::Field(_)))
}

/// A range step, which a goal carries only for a range formed at its use.
fn ranged(projections: &[GoalProjection]) -> bool {
    projections
        .iter()
        .any(|projection| matches!(projection, GoalProjection::Range(_)))
}

/// Whether a goal is one relation over atoms, so that its rendering is the
/// source text of a condition or a `requires` clause [GRAM-9].
pub(super) fn is_source_relation(goal: &GoalExpression) -> bool {
    let GoalExpression::Operation { row, arguments, .. } = goal else {
        return false;
    };
    let relation = match row {
        GoalOperation::Integer { operation, .. } => matches!(
            operation,
            CheckedIntegerOperation::Equal
                | CheckedIntegerOperation::NotEqual
                | CheckedIntegerOperation::Less
                | CheckedIntegerOperation::LessEqual
                | CheckedIntegerOperation::Greater
                | CheckedIntegerOperation::GreaterEqual
                | CheckedIntegerOperation::AddDefined
                | CheckedIntegerOperation::SubtractDefined
                | CheckedIntegerOperation::MultiplyDefined
                | CheckedIntegerOperation::DivideDefined
                | CheckedIntegerOperation::RemainderDefined
                | CheckedIntegerOperation::NegateDefined
                | CheckedIntegerOperation::AbsoluteDefined
                | CheckedIntegerOperation::ShiftLeftDefined
                | CheckedIntegerOperation::ShiftRightDefined
        ),
        GoalOperation::NumericConversion { mode, .. } => *mode == CheckedConversionMode::Defined,
        _ => false,
    };
    relation && arguments.iter().all(is_source_atom)
}

/// The shape of a goal over the `len` of ranges formed at the call: one
/// comparison whose operands are source atoms or such lengths, so that, with
/// each length written as its endpoint difference, it is the text of a
/// condition, or any other goal.
pub(super) fn range_goal_shape(goal: &GoalExpression) -> RangeGoalShape {
    let GoalExpression::Operation {
        row: GoalOperation::Integer { operation, .. },
        arguments,
        ..
    } = goal
    else {
        return RangeGoalShape::Other;
    };
    let operands = arguments.iter().all(|argument| {
        is_source_atom(argument)
            || matches!(
                argument,
                GoalExpression::Operation {
                    row: GoalOperation::ContainerMeasure { .. }
                        | GoalOperation::ArrayMeasure { .. }
                        | GoalOperation::BufferMeasure { .. },
                    arguments,
                    ..
                } if matches!(
                    arguments.as_slice(),
                    [GoalExpression::Datum(GoalDatum::Place { projections, .. })]
                        if ranged(projections)
                )
            )
    });
    match operation {
        _ if !operands => RangeGoalShape::Other,
        CheckedIntegerOperation::Equal | CheckedIntegerOperation::NotEqual => {
            RangeGoalShape::Equality
        }
        CheckedIntegerOperation::Less
        | CheckedIntegerOperation::LessEqual
        | CheckedIntegerOperation::Greater
        | CheckedIntegerOperation::GreaterEqual => RangeGoalShape::Comparison,
        _ => RangeGoalShape::Other,
    }
}

fn is_source_atom(expression: &GoalExpression) -> bool {
    match expression {
        GoalExpression::Datum(GoalDatum::Literal(_)) => true,
        GoalExpression::Datum(
            GoalDatum::NamedConst { projections, .. } | GoalDatum::Place { projections, .. },
        ) => spelled(projections),
        // A measure, and an admitted element read, render as the place a
        // source atom writes.
        GoalExpression::Operation {
            row:
                GoalOperation::ContainerMeasure { .. }
                | GoalOperation::ArrayMeasure { .. }
                | GoalOperation::BufferMeasure { .. }
                | GoalOperation::ArrayIndex { .. }
                | GoalOperation::BufferIndex { .. }
                | GoalOperation::RunIndex { .. },
            arguments,
            ..
        } => arguments.iter().all(is_source_atom),
        _ => false,
    }
}

/// The total spellings [OP-2]'s mode table gives an exact operation whose
/// canonical goal is this `.defined` query.
pub(super) fn total_forms(goal: &GoalExpression) -> Option<&'static str> {
    let GoalExpression::Operation {
        row: GoalOperation::Integer { operation, .. },
        ..
    } = goal
    else {
        return None;
    };
    Some(match operation {
        CheckedIntegerOperation::AddDefined => "`+wrap`, `+checked` or `+sat`",
        CheckedIntegerOperation::SubtractDefined => "`-wrap`, `-checked` or `-sat`",
        CheckedIntegerOperation::MultiplyDefined => "`*wrap`, `*checked` or `*sat`",
        CheckedIntegerOperation::DivideDefined => "`/checked`",
        CheckedIntegerOperation::RemainderDefined => "`%checked`",
        CheckedIntegerOperation::NegateDefined => "`ineg.wrap` or `ineg.checked`",
        CheckedIntegerOperation::AbsoluteDefined => "`iabs.wrap` or `iabs.checked`",
        CheckedIntegerOperation::ShiftLeftDefined => "`ishl.wrap`",
        CheckedIntegerOperation::ShiftRightDefined => "`ishr.wrap`",
        _ => return None,
    })
}

/// One rejected goal: its disposition, what its terms are, its source text
/// when that text is a condition, and the enclosing function a `requires`
/// would be added to.
pub(super) struct GoalCase<'a> {
    pub(super) disposition: Disposition,
    pub(super) terms: GoalTerms,
    /// The goal reads through a reference parameter [EFF-2].
    pub(super) referenced: bool,
    /// The goal reads a value a user call returned [FN-9].
    pub(super) called: bool,
    /// The goal as the payload renders it.
    pub(super) text: &'a str,
    /// Whether `text` is the source of one condition over atoms.
    pub(super) condition: bool,
    pub(super) function: &'a str,
}

impl GoalCase<'_> {
    /// `requires` on the enclosing function, and the guard; the two routes
    /// every unproved goal over unwritten parameters has.
    fn parameter_routes(&self, construct: &str, intent: &str) -> String {
        let guard = self.guard(construct, intent);
        if self.condition {
            format!(
                "add `requires {};` to the `contract` of `{}`, which each caller then establishes; or {guard}",
                self.text, self.function,
            )
        } else {
            format!(
                "state the relation over the parameters of `{}` as a `requires` in its `contract`, which each caller then establishes; or {guard}",
                self.function,
            )
        }
    }

    /// The guard alternative, for a program whose intent is to skip the
    /// construct when the goal fails. Unlike a clause or an invariant, a
    /// condition is executable code, so a read it makes through a reference
    /// parameter is one the row must declare [EFF-2].
    fn guard(&self, construct: &str, intent: &str) -> String {
        let condition = if self.condition {
            format!("`if {}`", self.text)
        } else {
            String::from("an `if` whose condition establishes it")
        };
        self.guard_with(construct, &condition, intent)
    }

    /// The guard alternative with its condition spelled by the caller.
    fn guard_with(&self, construct: &str, condition: &str, intent: &str) -> String {
        let row = if self.referenced {
            ", adding to the effect row any read that condition makes which the row does not yet declare"
        } else {
            ""
        };
        format!("guard the {construct} with {condition} {intent}{row}")
    }

    /// The route of an unproved goal that reads a value no fact names: once
    /// a `let` binds that value the goal is over a term, and the
    /// rejection that follows names the routes for it.
    fn unnamed_route(&self, construct: &str) -> String {
        format!(
            "`{}` reads a value no fact can name until a `let` binds it: bind that value with one preceding `let`, use the binding in the {construct}, and establish the relation over the binding",
            self.text
        )
    }

    /// The proof routes of an unproved goal over computed values. Each
    /// states the condition it needs, which the checker cannot decide: a
    /// written certificate or a callee relation the program does not have yet.
    fn computed_routes(&self, construct: &str, intent: &str) -> String {
        let guard = self.guard(construct, intent);
        // A callee's `ensures` bounds only a value that callee returned.
        let callee = if self.called {
            "; when the callee whose result it reads can prove the bound, state it in that callee's `ensures`"
        } else {
            ""
        };
        format!(
            "when facts that reach the {construct} imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes){callee}; or {guard}"
        )
    }
}

const SKIP: &str = "where skipping it is the intended behavior";

/// [REF-4] one range an argument formed at the call whose `len` an unproved
/// requirement reads, as its repair spells it.
pub(super) struct RangeLength {
    /// The range as the instantiated goal spells it.
    pub(super) range: String,
    /// Each endpoint's source spelling, and whether it is a literal, const
    /// or binding, which a relation can name.
    pub(super) start: (String, bool),
    pub(super) end: (String, bool),
    /// The goal with each such `len` written as its endpoint difference.
    pub(super) difference_goal: Option<String>,
}

impl RangeLength {
    /// The range's length in source terms: its end when it starts at zero,
    /// otherwise the exact difference of its endpoints [REF-4].
    fn difference(&self) -> String {
        if self.start.0 == "0_u64" {
            self.end.0.clone()
        } else {
            format!("{} - {}", self.end.0, self.start.0)
        }
    }
}

/// The shape of an unproved goal over range lengths, which selects the
/// route its repair can carry out.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RangeGoalShape {
    /// One ordering or equality whose operands are atoms or such lengths:
    /// written with each length as its difference, it is a condition.
    Comparison,
    /// One `==` or `!=` of that shape. With a length that is the difference
    /// of two distinct endpoints it has no difference-bound form [ENT-4].
    Equality,
    /// Any other goal.
    Other,
}

/// [FN-8, REF-4] an unproved requirement over the `len` of ranges formed at
/// the call. Each length is the difference of its endpoints, so the
/// requirement bounds that difference; the routes name it, and ask for it
/// computed with the exact subtraction, because a `-wrap` difference is a
/// fresh value no relation ties to the endpoints [ENT-3].
fn range_length_routes(
    case: &GoalCase<'_>,
    ranges: &[RangeLength],
    shape: RangeGoalShape,
) -> String {
    let bind = "or bind the range with one preceding `let`, use the binding in the call, and establish the relation over the binding";
    let wrap = "a `-wrap` difference carries no relation to the range's length";
    // An endpoint no relation names: a field or element read, say. Its
    // range's length is a difference of values no fact names until a `let`
    // binds each of them.
    if let Some(range) = ranges.iter().find(|range| !range.start.1 || !range.end.1) {
        let unnamed = [&range.start, &range.end]
            .into_iter()
            .filter(|endpoint| !endpoint.1)
            .map(|endpoint| format!("`{}`", endpoint.0))
            .collect::<Vec<_>>();
        let (subject, pronoun) = if unnamed.len() == 1 {
            (
                format!("endpoint {} is not a binding", unnamed.join("")),
                "it",
            )
        } else {
            (
                format!("endpoints {} are not bindings", unnamed.join(" and ")),
                "each",
            )
        };
        return format!(
            "`{}.len` is the difference of its endpoints [REF-4], and its {subject}, so no fact names that difference: copy {pronoun} into a `let` binding before the call and form the range from the bindings, then, where the start is at most the end, bind the difference of the bindings with the exact `-` ({wrap}) and establish the requirement over it",
            range.range,
        );
    }
    let lengths = ranges
        .iter()
        .map(|range| format!("`{}.len` is `{}`", range.range, range.difference()))
        .collect::<Vec<_>>()
        .join(" and ");
    let goal = ranges
        .first()
        .and_then(|range| range.difference_goal.as_deref())
        .unwrap_or(case.text);
    let subtracted = ranges
        .iter()
        .filter(|range| range.start.0 != "0_u64")
        .collect::<Vec<_>>();
    match (shape, subtracted.as_slice()) {
        (RangeGoalShape::Comparison | RangeGoalShape::Equality, []) => format!(
            "{lengths} [REF-4], so this call needs `{goal}`: guard the call with `if {goal}` {SKIP}; {bind}"
        ),
        (RangeGoalShape::Equality, [range, ..]) => format!(
            "{lengths} [REF-4], so this call needs `{goal}`, and an equality over the difference of two distinct endpoints has no difference-bound form [ENT-4]: where the start is at most the end, bind each such difference with the exact `-`, as `let width = {};` ({wrap}), bind its range with one preceding `let`, as `let part = &{};`, and pass `&part^[0_u64..width]` in its place, whose length is `width` itself; then establish the requirement over those bindings",
            range.difference(),
            range.range,
        ),
        (RangeGoalShape::Comparison, [range]) => {
            let difference = range.difference();
            let guarded = goal.replacen(&difference, "width", 1);
            format!(
                "{lengths} [REF-4], so this call needs `{goal}`: where `{} <= {}` holds, bind the difference with the exact `-`, `let width = {difference};` ({wrap}), and guard the call with `if {guarded}` {SKIP}; {bind}",
                range.start.0, range.end.0,
            )
        }
        _ => format!(
            "{lengths} [REF-4], so this call needs `{goal}`: where each start is at most its end, bind each difference with the exact `-` ({wrap}) and establish the requirement over those bindings before the call; {bind}"
        ),
    }
}

/// [FN-8] an ordinary call's requirement. `ranges` are the ranges formed at
/// the call whose `len` the goal reads, and `shape` is the goal's shape
/// over them.
pub(super) fn call_requirement(
    case: &GoalCase<'_>,
    ranges: &[RangeLength],
    shape: RangeGoalShape,
) -> String {
    match (case.disposition, case.terms) {
        (Disposition::Unproved, GoalTerms::RangeLength) if !ranges.is_empty() => {
            range_length_routes(case, ranges, shape)
        }
        (Disposition::Refuted, _) => format!(
            "`{}` is false for the values that reach this call, so no fact can establish it here: pass arguments that satisfy it, or change the statements or requirements that fix those values",
            case.text
        ),
        (Disposition::Unproved, GoalTerms::CallArgument(argument)) => format!(
            "argument #{argument} is evaluated inside the call, where no fact names its value: bind it with one preceding `let`, establish the requirement over that binding, and pass the binding, borrowing it when the parameter is a reference"
        ),
        (Disposition::Unproved, GoalTerms::Unnamed | GoalTerms::RangeLength) => {
            case.unnamed_route("call")
        }
        (Disposition::Unproved, GoalTerms::Parameters) => case.parameter_routes("call", SKIP),
        (Disposition::Unproved, GoalTerms::Computed) => format!(
            "`{}` is not proved before this call: {}",
            case.text,
            case.computed_routes("call", SKIP)
        ),
    }
}

/// [TYPE-11] a construction's type invariant over its field operands.
pub(super) fn construction_invariant(case: &GoalCase<'_>) -> String {
    match case.disposition {
        Disposition::Refuted => format!(
            "`{}` is false for the operands this construction receives: construct the value from operands that satisfy it, or change the statements that fix those operands",
            case.text
        ),
        Disposition::Unproved => format!(
            "`{}` is not proved before this construction: establish it over the operands first, with a `requires`, an `if` or an `invariant` over the locals they are read from, and then construct the value",
            case.text
        ),
    }
}

/// [TYPE-11] a type invariant a `const` initializer's construction owes,
/// which its written field values decide exactly.
pub(super) fn constant_construction_invariant(text: &str) -> String {
    format!(
        "`{text}` is false for the field values this constant writes: write values that satisfy it, or declare the constant with a struct that states no such invariant"
    )
}

/// [TYPE-11] a type invariant of an object's state at an edge that leaves
/// an atomic block.
pub(super) fn atomic_exit_invariant(case: &GoalCase<'_>) -> String {
    match case.disposition {
        Disposition::Refuted => format!(
            "`{}` is false where the block leaves the object's state: restore it before this edge, or leave the block where it holds",
            case.text
        ),
        Disposition::Unproved => format!(
            "`{}` is not proved where the block leaves the object's state: restore it before this edge, writing the fields it relates so the block shows it holds, or prove it with an `invariant` whose `use` steps name the facts it follows from",
            case.text
        ),
    }
}

/// [FN-9] a normal-result relation at one selected return; `called` says
/// whether the returned value reads a value a user call returned.
pub(super) fn postcondition(disposition: Disposition, called: bool) -> &'static str {
    match (disposition, called) {
        (Disposition::Refuted, _) => {
            "the value this `return` delivers makes the postcondition false: return a value that satisfies it, state a postcondition this return satisfies, or change the requirements that fix the returned value"
        }
        (Disposition::Unproved, true) => {
            "the postcondition is not proved where this `return` delivers its value: add a `requires` over the parameters the value is computed from, prove the bound before the return with an `invariant` whose `use` steps name the facts it follows from, state it in the `ensures` of the callee whose result the value reads when that callee can prove it, or state a postcondition the body proves"
        }
        (Disposition::Unproved, false) => {
            "the postcondition is not proved where this `return` delivers its value: add a `requires` over the parameters the value is computed from, prove the bound before the return with an `invariant` whose `use` steps name the facts it follows from, or state a postcondition the body proves"
        }
    }
}

/// [FN-9, ERR-3] a relation at a propagated error exit, which returns the
/// propagated outcome with the state the body has reached there.
pub(super) fn propagated_postcondition(disposition: Disposition) -> &'static str {
    match disposition {
        Disposition::Refuted => {
            "the state this `propagate` leaves the function with makes the postcondition false: restore the places it relates before the `propagate`, or state a postcondition every exit satisfies"
        }
        Disposition::Unproved => {
            "the postcondition is not proved where this `propagate` leaves the function: establish it before the `propagate`, writing the places it relates or proving it with an `invariant` whose `use` steps name the facts it follows from, or state a postcondition every exit satisfies"
        }
    }
}

/// [FN-9] a clause whose route no normal return selects.
pub(super) const NO_SELECTED_EXIT: &str = "no `return` of this function delivers a value this clause's route selects: return such a value on some path, or delete the clause";

/// [RANGE-3] a range postcondition no exit the range walk reaches selects.
pub(super) const NO_SELECTED_RANGE_EXIT: &str = "no `return` the range judgment reaches delivers a value this clause's route selects: return such a value on a path the walk reaches, or delete the clause";

/// [OP-2] an exact integer operation's `.defined` domain.
pub(super) fn integer_domain(case: &GoalCase<'_>, forms: Option<&str>) -> String {
    let total = forms.map_or_else(String::new, |forms| format!("; or write the {forms} form"));
    match (case.disposition, case.terms) {
        (Disposition::Refuted, _) => {
            let total = forms.map_or_else(String::new, |forms| {
                format!(", or write the {forms} form for the result the program intends")
            });
            format!(
                "the operands that reach this operation make `{}` false, so the exact operation cannot execute here: change the operands or their type{total}",
                case.text
            )
        }
        (Disposition::Unproved, GoalTerms::Parameters) => {
            format!("{}{total}", case.parameter_routes("operation", SKIP))
        }
        (
            Disposition::Unproved,
            GoalTerms::Unnamed | GoalTerms::RangeLength | GoalTerms::CallArgument(_),
        ) => {
            format!("{}{total}", case.unnamed_route("operation"))
        }
        (Disposition::Unproved, GoalTerms::Computed) => format!(
            "`{}` is not proved here: {}{total}",
            case.text,
            case.computed_routes("operation", SKIP)
        ),
    }
}

/// [OP-6] a bare conversion's domain. `checked` spells the total form and
/// `integer_source` whether an affine invariant can bound the operand.
pub(super) fn conversion_domain(
    case: &GoalCase<'_>,
    checked: &str,
    destination: &str,
    integer_source: bool,
) -> String {
    let fallible = format!("use `{checked}` and handle its `Err`");
    match (case.disposition, case.terms) {
        (Disposition::Refuted, _) => format!(
            "the value that reaches this conversion is outside `{destination}`: convert a value `{destination}` holds, choose a destination type that holds this one, or {fallible}"
        ),
        (Disposition::Unproved, GoalTerms::Parameters) => {
            format!(
                "{}; or {fallible}",
                case.parameter_routes("conversion", SKIP)
            )
        }
        (
            Disposition::Unproved,
            GoalTerms::Unnamed | GoalTerms::RangeLength | GoalTerms::CallArgument(_),
        ) => {
            format!("{}; or {fallible}", case.unnamed_route("conversion"))
        }
        (Disposition::Unproved, _) if integer_source => format!(
            "`{}` is not proved here: {}; or {fallible}",
            case.text,
            case.computed_routes("conversion", SKIP)
        ),
        (Disposition::Unproved, _) => format!(
            "`{}` is not proved here, and no fact bounds a float operand: {}, or {fallible}",
            case.text,
            case.guard("conversion", SKIP)
        ),
    }
}

/// [OP-4] a subscript's bound.
pub(super) fn bounds(case: &GoalCase<'_>, constant_offset: bool) -> String {
    match (case.disposition, case.terms) {
        (Disposition::Refuted, _) => format!(
            "`{}` is false where this access executes: {}",
            case.text,
            refuted_index(constant_offset, false)
        ),
        (Disposition::Unproved, GoalTerms::Parameters) => case.parameter_routes("access", SKIP),
        (
            Disposition::Unproved,
            GoalTerms::Unnamed | GoalTerms::RangeLength | GoalTerms::CallArgument(_),
        ) => case.unnamed_route("access"),
        (Disposition::Unproved, GoalTerms::Computed) => format!(
            "`{}` is not proved here: {}",
            case.text,
            case.computed_routes("access", SKIP)
        ),
    }
}

/// One call on a path to a failed subscript whose row reached its measure
/// and removed the facts that, put back, prove the bound [ENT-5].
pub(super) struct KillingCall {
    pub(super) callee: String,
    pub(super) line: u64,
    pub(super) written: String,
}

/// [OP-4] a subscript's bound that the facts some calls' rows removed would
/// prove: the calls are named, and where each leaves the length unchanged,
/// a narrower row entry or a postcondition stating the length unchanged
/// keeps those facts; the guard stays the alternative where the length may
/// change [DIAG-1].
pub(super) fn bounds_after_kill(
    case: &GoalCase<'_>,
    measure: &str,
    calls: &[KillingCall],
) -> String {
    let one = calls.len() == 1;
    let sites = calls
        .iter()
        .map(|call| format!("to `{}` at line {}", call.callee, call.line))
        .collect::<Vec<_>>()
        .join(" and ");
    let written = calls
        .iter()
        .map(|call| format!("`{}`", call.written))
        .collect::<Vec<_>>()
        .join(" and ");
    let mut callees = calls
        .iter()
        .map(|call| call.callee.as_str())
        .collect::<Vec<_>>();
    callees.sort_unstable();
    callees.dedup();
    let who = callees
        .iter()
        .map(|callee| format!("`{callee}`"))
        .collect::<Vec<_>>()
        .join(" and ");
    let (call, rows, leaves, its) = if one {
        ("call", "its row, which writes", "leaves", "its")
    } else if callees.len() == 1 {
        ("calls", "their rows, which write", "leaves", "its")
    } else {
        ("calls", "their rows, which write", "leave", "their")
    };
    format!(
        "`{}` is not proved here, but facts about `{measure}` that held before the {call} {sites} would prove it, and {rows} {written}, removed them: where {who} {leaves} `{measure}` unchanged, narrow the entry of {its} row that covers it to the paths {its} body writes, or state `{measure}` unchanged in {its} `ensures`; or {}",
        case.text,
        case.guard("access", SKIP)
    )
}

/// [OP-4] a subscript of a place a contract clause forms [ENT-2, FN-8]. The
/// place is formed at body entry in the state the requirements written before
/// it build, so an earlier requirement establishes the bound; a clause
/// evaluates nothing, so no guard can skip it.
pub(super) fn clause_bounds(case: &GoalCase<'_>, constant_offset: bool) -> String {
    match case.disposition {
        Disposition::Refuted => format!(
            "`{}` is false where this place is formed: {}",
            case.text,
            refuted_index(constant_offset, true)
        ),
        Disposition::Unproved => format!(
            "add `requires {};` to the `contract` of `{}` ahead of the requirement that forms this place, which each caller then establishes",
            case.text, case.function
        ),
    }
}

/// The alternatives of a refuted subscript. A longer storage holds only an
/// index that does not grow with it: an offset such as `r.len` stays out of
/// range at every length, so a constant offset is given that route and any
/// other offset the facts that fix it, which in a clause are the requirements
/// written before it [FN-8].
fn refuted_index(constant_offset: bool, clause: bool) -> &'static str {
    match (constant_offset, clause) {
        (true, _) => "index within the storage, or give the storage a length that holds this index",
        (false, false) => {
            "index within the storage, or change the statements or requirements that fix the index"
        }
        (false, true) => {
            "index within the storage, or change the requirements before this one that fix the index"
        }
    }
}

/// [OP-9] an allocation's size. The residual's bound is the language's
/// ceiling for the element type, which the selected target's layout then
/// qualifies again with the retained proved bound [STOR-6]: every supported
/// target admits a smaller count, so a program that states the ceiling passes
/// OP-9 and stops at target layout. No route offers it as the bound to write;
/// each asks for the largest count the program needs.
pub(super) fn allocation_fit(case: &GoalCase<'_>) -> String {
    const REFUSE: &str = "where refusing a larger count is the intended behavior";
    // The residual is `count <= ceiling`, the count as its source spells it.
    let (count, ceiling) = case
        .text
        .rsplit_once(" <= ")
        .unwrap_or((case.text, case.text));
    let limit = format!(
        "`{ceiling}` is the language's limit for this element type, not a bound to write: the selected target admits a smaller count, so a bound at or near that limit stops at target layout [STOR-6]"
    );
    let guard = case.guard_with("allocation", &format!("`if {count} <= N`"), REFUSE);
    match (case.disposition, case.terms) {
        (Disposition::Refuted, _) => format!(
            "`{}` is false, so this allocation cannot be formed: request the count the program needs. {limit}",
            case.text
        ),
        (Disposition::Unproved, GoalTerms::Parameters) => format!(
            "with N the largest count the program needs, add `requires {count} <= N;` to the `contract` of `{}`, which each caller then establishes; or {guard}. {limit}",
            case.function
        ),
        (
            Disposition::Unproved,
            GoalTerms::Unnamed | GoalTerms::RangeLength | GoalTerms::CallArgument(_),
        ) => format!(
            "`{count}` reads a value no fact can name until a `let` binds it: bind that value with one preceding `let`, use the binding in the allocation, and bound the binding by the largest count the program needs. {limit}"
        ),
        (Disposition::Unproved, GoalTerms::Computed) => {
            let callee = if case.called {
                "; when the callee whose result it reads can prove that bound, state it in the callee's `ensures`"
            } else {
                ""
            };
            format!(
                "`{count}` is not bounded here: with N the largest count the program needs, when facts that reach the allocation imply `{count} <= N`, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes){callee}; or {guard}. {limit}"
            )
        }
    }
}

/// [STOR-6] the fix a target-layout stop carries when an allocating call's
/// retained count bound exceeds the largest count the selected target
/// admits. The stop is no rejection and cites no rule [DIAG-1], but its words
/// live beside [OP-9]'s because both send the writer to the same count: the
/// target qualifies the bound the checked program proves, so the fix is a
/// tighter proof where the count is computed. Target qualification does not
/// know what the count reads, so the sentence offers each form that bounds
/// it; `limit` is the largest count the target admits.
pub(crate) fn target_allocation_count(count: &str, limit: u64) -> String {
    format!(
        "with N the largest count the program needs, at most {limit}, bound `{count}` by N before this call: add `requires {count} <= N;` to the `contract` of the function whose parameter it is, which each caller then establishes; state the bound in the `ensures` of the function whose result it is; or guard the allocation with `if {count} <= N` where refusing a larger count is the intended behavior"
    )
}

/// [REF-4] one range-formation conjunct.
pub(super) fn range_formation(case: &GoalCase<'_>) -> String {
    match (case.disposition, case.terms) {
        (Disposition::Refuted, _) => format!(
            "`{}` is false where this range is formed: choose endpoints that satisfy it",
            case.text
        ),
        (Disposition::Unproved, GoalTerms::Parameters) => case.parameter_routes("range", SKIP),
        (
            Disposition::Unproved,
            GoalTerms::Unnamed | GoalTerms::RangeLength | GoalTerms::CallArgument(_),
        ) => case.unnamed_route("range"),
        (Disposition::Unproved, GoalTerms::Computed) => format!(
            "`{}` is not proved here: {}",
            case.text,
            case.computed_routes("range", SKIP)
        ),
    }
}

/// [OP-14] `free_empty`'s requirement that the window is empty. Skipping the
/// consume does not discharge a linear window: it must be emptied and
/// consumed, not abandoned at scope exit [PROV-6].
pub(super) fn empty_run_release(case: &GoalCase<'_>) -> String {
    const EMPTY: &str = "take every element out and consume it before this call, so that its zero length is established here";
    match (case.disposition, case.terms) {
        (Disposition::Refuted, _) => String::from(
            "the window still holds elements here: take every element out and consume it before `free_empty`",
        ),
        (Disposition::Unproved, GoalTerms::Parameters) if case.condition => format!(
            "add `requires {};` to the `contract` of `{}`, which each caller then establishes, or {EMPTY}",
            case.text, case.function
        ),
        (Disposition::Unproved, GoalTerms::Parameters) => format!(
            "state the window's zero length over the parameters of `{}` as a `requires` in its `contract`, which each caller then establishes, or {EMPTY}",
            case.function
        ),
        (Disposition::Unproved, _) => format!("`{}` is not proved here: {EMPTY}", case.text),
    }
}

/// [INV-1] a blockless local invariant's target.
pub(super) fn local_invariant(disposition: Disposition, name: &str) -> String {
    match disposition {
        Disposition::Refuted => format!(
            "`{name}` is false where it is stated: correct the relation, or state one that the facts reaching it imply"
        ),
        Disposition::Unproved => format!(
            "`{name}` is not proved from the facts that reach it: establish them before it, add `use` steps naming the facts it follows from, or weaken it"
        ),
    }
}

/// [INV-1] a loop invariant's base judgment on entry to the loop.
pub(super) fn loop_invariant_base(disposition: Disposition, name: &str) -> String {
    match disposition {
        Disposition::Refuted => format!(
            "`{name}` is false on entry to the loop: correct it, or change the values the loop starts from"
        ),
        Disposition::Unproved => format!(
            "`{name}` is not proved on entry to the loop: establish before the loop the facts it follows from, or weaken or correct it"
        ),
    }
}

/// [INV-1] a loop invariant's backedge judgment at the next header.
pub(super) fn loop_invariant_backedge(disposition: Disposition, name: &str) -> String {
    match disposition {
        Disposition::Refuted => format!(
            "an iteration makes `{name}` false at the next loop header: correct it, or change the body so that every iteration preserves it"
        ),
        Disposition::Unproved => format!(
            "`{name}` is not proved preserved at the next loop header: strengthen the invariant prefix, weaken or correct it, or establish in the body the facts from which every reachable fallthrough preserves it"
        ),
    }
}

/// [EFF-5] a call whose two substituted entries no admitted family separated,
/// by the first position the checker handed over and by whether one argument
/// supplies both entries. The facts may already refute the separation, so
/// proving it is offered only where it can hold; changing what the call
/// passes works either way [DIAG-1]. A position beside a window's `next` or
/// `free` is separated by a bound on the window's length [WIN-2], an index
/// beside a range by lying outside it and two ranges by one ending before the
/// other starts [OWN-7], not by differing. When one argument
/// supplies both entries, the callee's row can instead name their common path
/// once.
pub(super) fn call_separation(
    position: Option<CheckedCallSeparationPositions>,
    one_argument: bool,
) -> &'static str {
    use CheckedCallSeparationPositions as Positions;
    match (position, one_argument) {
        (Some(Positions::Live(_)), false) => {
            "when the index can be below the window's length here, prove that before this call; otherwise pass an index this call proves below it"
        }
        (Some(Positions::Live(_)), true) => {
            "when the index can be below the window's length here, prove that before this call; otherwise pass an index this call proves below it, or replace the callee's row entries at or below their common path with one `writes` entry of that path"
        }
        (Some(Positions::IndexOutsideRange(..)), false) => {
            "when the index can lie outside the range here, prove before this call that it is below the range's start or at or after its end; otherwise pass positions this call proves apart"
        }
        (Some(Positions::IndexOutsideRange(..)), true) => {
            "when the index can lie outside the range here, prove before this call that it is below the range's start or at or after its end; otherwise pass positions this call proves apart, or replace the callee's row entries at or below their common path with one `writes` entry of that path"
        }
        (Some(Positions::RangeWithinLength(_)), false) => {
            "when the range can end at or below the window's length here, prove that before this call; otherwise pass a range this call proves ends there"
        }
        (Some(Positions::RangeWithinLength(_)), true) => {
            "when the range can end at or below the window's length here, prove that before this call; otherwise pass a range this call proves ends there, or replace the callee's row entries at or below their common path with one `writes` entry of that path"
        }
        (Some(Positions::Ranges(..)), false) => {
            "when the two ranges can lie apart here, prove before this call that one ends at or before the other starts, or that one is empty; otherwise pass ranges this call proves apart"
        }
        (Some(Positions::Ranges(..)), true) => {
            "when the two ranges can lie apart here, prove before this call that one ends at or before the other starts, or that one is empty; otherwise pass ranges this call proves apart, or replace the callee's row entries at or below their common path with one `writes` entry of that path"
        }
        (_, false) => {
            "when the two positions can differ here, prove them distinct before this call; otherwise pass places this call proves do not overlap"
        }
        (_, true) => {
            "when the two positions can differ here, prove them distinct before this call; otherwise pass positions this call proves distinct, or replace the callee's row entries at or below their common path with one `writes` entry of that path"
        }
    }
}

/// Where an opaque struct that a constructor `call` or a destructuring `let`
/// names comes from, which selects the repair of its refusal [TYPE-2].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OpaqueStruct {
    /// A host handle: a standard library module declares it with no fields,
    /// and only a host function forms one [PRE-2]. `linear` when its
    /// declaration writes `nodrop`, so that it leaves a scope only by moving
    /// out [PROV-6].
    HostHandle { linear: bool },
    /// A host module's opaque struct with fields, `Instant`: only a host
    /// function forms one, and its fields are private to a module no program
    /// writes in [PRE-2, MOD-6].
    HostValue,
    /// An opaque struct the program declares, which never has a value.
    Program,
}

/// A program's own opaque struct has a value once its declaration drops the
/// modifier [TYPE-2]. Outside the declaring module, the construct then meets
/// [MOD-5]'s judgment of the fields it gives or binds, whose own repair names
/// them.
const PROGRAM_OPAQUE_STRUCT: &str = "no value of an opaque struct the program declares is ever formed [TYPE-2]: remove `opaque` from its declaration";

/// [TYPE-2] a constructor `call` naming an opaque struct a module declares.
/// Every host handle comes from a host function of its module, or from the
/// program's entry in `std::process::Inputs` [PRE-2], so the one alternative
/// names both sources.
pub(super) fn opaque_struct_constructed(opaque: OpaqueStruct) -> &'static str {
    match opaque {
        OpaqueStruct::HostHandle { .. } => {
            "a host handle is formed only by a host function [PRE-2]: replace this construction with a handle that a function of its module returns or that the program's entry receives"
        }
        OpaqueStruct::HostValue => {
            "a host module's opaque value is formed only by a host function [PRE-2]: replace this construction with a value that a function of its module returns"
        }
        OpaqueStruct::Program => PROGRAM_OPAQUE_STRUCT,
    }
}

/// [TYPE-2] a destructuring `let` naming an opaque struct a module declares,
/// consuming a place that `owned` says names owned storage directly. A host
/// handle has nothing to take apart, so the statement goes; a `nodrop` one
/// this function owns is then still owed its closing call [PROV-6], which
/// nothing makes through a reference or an element [OWN-1, WIN-3].
pub(super) fn opaque_struct_taken_apart(opaque: OpaqueStruct, owned: bool) -> &'static str {
    match opaque {
        OpaqueStruct::HostHandle { linear: true } if owned => {
            "a host handle has no fields to take apart [PRE-2], and a `nodrop` one leaves its scope only by moving out [PROV-6]: replace this statement with a call to the function of its module that closes the handle, which also takes a `HandleFactory` reference"
        }
        OpaqueStruct::HostHandle { .. } => {
            "a host handle has no fields to take apart [PRE-2]: remove this statement"
        }
        OpaqueStruct::HostValue => {
            "a host module's opaque value keeps its fields private to its module [PRE-2, MOD-6]: read it through the functions of its module instead of taking it apart"
        }
        OpaqueStruct::Program => PROGRAM_OPAQUE_STRUCT,
    }
}

/// What a cell's content is, which selects how a statement that takes the
/// cell apart reaches the content instead [TYPE-9].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CellContent {
    /// A copy value, which is read in place and never moved [OWN-1].
    Copy,
    /// A value without copy other than a runtime-capacity shape, which moves
    /// out and frees the cell [WIN-3].
    Owned,
    /// A runtime-capacity shape, which never leaves its cell [TYPE-9].
    RuntimeCapacity,
    /// A content not read as a copy, in a cell a reference or an element
    /// reaches, which nothing moves out of [OWN-1, WIN-3], so it is used in
    /// place.
    InPlace,
}

/// [TYPE-2, TYPE-9] a destructuring `let` naming `Box`. `cell` is the
/// consumed place as a read of it is written and `binder` the name the
/// statement binds, when it binds one. `content` is `None` when the place
/// selects no cell the checker can type.
pub(super) fn cell_taken_apart(
    content: Option<CellContent>,
    cell: &str,
    binder: Option<&str>,
) -> String {
    const INNER: &str = "a cell's content is its member `inner` [TYPE-9]";
    match (content, binder) {
        (Some(CellContent::Copy), Some(name)) => {
            format!("{INNER}: replace this statement with `let {name} = {cell}.inner;`")
        }
        (Some(CellContent::Owned), Some(name)) => format!(
            "{INNER}: replace this statement with `let {name} = move {cell}.inner;`, which frees the cell [WIN-3]"
        ),
        (Some(CellContent::RuntimeCapacity), Some(name)) => format!(
            "{INNER}, and a runtime-capacity content never leaves it: remove this statement, and write `{cell}.inner` where `{name}` is used and `move {cell}` where `{name}` is moved [OP-14]"
        ),
        (Some(CellContent::InPlace), Some(name)) => format!(
            "{INNER}, and nothing moves out of a cell a reference or an element reaches [OWN-1, WIN-3]: when `{name}` is never moved, remove this statement and write `{cell}.inner` where `{name}` is used"
        ),
        (None, _) | (_, None) => format!("{INNER}: remove this statement"),
    }
}

/// [TYPE-9] runtime-capacity content stays in its Box. Only windows have
/// the empty-storage consume [OP-14], and only content with drop can be
/// released at scope exit [PROV-6]. The caller supplies the typed Box path.
pub(super) fn runtime_content_move(cell: &str, window: bool, droppable: bool) -> String {
    let mut repair = format!(
        "replace `move {cell}.inner` with `move {cell}` and keep the receiving value boxed, accessing its content through `.inner`"
    );
    if droppable {
        repair.push_str(&format!(
            "; if the move was intended only to release the content, remove it and let `{cell}` release at scope exit"
        ));
    }
    if window {
        repair.push_str(&format!(
            "; to release the window explicitly instead, take every element out and consume it, establish `{cell}.inner.len == 0_u64`, and call `free_empty(window: move {cell})` [OP-14]"
        ));
    }
    repair
}

/// [TYPE-2, TYPE-9] a destructuring statement naming a storage shape reads
/// its readonly fields instead. Each pair retains the written field and its
/// binder; a rest marker introduces no binding and no read. `None` means
/// the actual operand does not establish all the written measure fields.
pub(super) fn storage_taken_apart(storage: &str, fields: Option<&[(String, String)]>) -> String {
    const FIELDS: &str = "storage shapes expose their measures as readonly fields [TYPE-9]";
    let Some(fields) = fields else {
        return format!(
            "{FIELDS}: remove this statement, keep using `{storage}` directly, and replace uses of its bindings with the values the program needs"
        );
    };
    if fields.is_empty() {
        return format!("{FIELDS}: remove this statement and keep using `{storage}` directly");
    }
    let reads = fields
        .iter()
        .map(|(field, binder)| format!("let {binder} = {storage}.{field};"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{FIELDS}: when `{storage}` is a valid readable place, replace this statement with `{reads}` and extend the enclosing effect row to cover any reads through reference parameters [EFF-2]; otherwise remove this statement and replace uses of its bindings with the values the program needs"
    )
}
