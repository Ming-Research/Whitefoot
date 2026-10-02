//! [SHARE-3] which statements holding a map's state reach its entries only
//! under keys that are known before their block runs, and the statements
//! that compute those keys.
//!
//! A statement holding a map's state takes effect at one point, and whether
//! it excludes a statement on an entry its block never reaches is not
//! observable. So when the block reaches the map only through
//! `atomic e = &s^[key] { ... }` and every such key can be computed first,
//! lowering collects the keys, and the runtime holds those entries and no
//! others (`wf_cmap_hold_set`). The block then runs once, as written, on
//! entries it already holds.
//!
//! The keys are computed by a *twin* of the block: its counted loops and
//! matches that contain such a statement, the `let`s their keys, bounds and
//! scrutinees read, and each such statement with an empty block. The twin
//! is sound when it computes at least the keys the block reaches. This
//! module answers a twin only when all of the following hold, and a
//! statement without a twin holds the whole map as before:
//!
//! - the block names the held state only as the target of such statements;
//! - no `return`, `break`, propagating `let` or `give` leaves any part of
//!   the block;
//! - a kept expression owes nothing the twin might not have: its integer
//!   operations and conversions answer for every operand, its calls have no
//!   `requires`, and a range it forms is narrowed to its source's bounds
//!   when the twin is lowered. The block proved such obligations under
//!   facts the twin, which drops the block's other statements, may lack: a
//!   guard arm that never returns, a call that does not. Where the block
//!   does form the range the narrowing changes nothing, so the twin still
//!   collects the block's key;
//! - no general loop contains such a statement, since a twin without the
//!   loop's exits would not end;
//! - the block runs at most one statement on a shared object
//!   ([`one_object_statement`]), and so does every other block of the
//!   program that holds an entry or a map's state
//!   ([`runs_object_sections`]): a block that runs two is a section that no
//!   statement on the map enters, or sees the middle of, only while whole
//!   maps are held;
//! - every kept expression is a constant, a binding read, a field of one,
//!   such an integer operation or conversion, a float or boolean operation,
//!   a range's measure, a range over a range or over a constant, or a call
//!   to a function that
//!   waits for nothing, writes nothing, allocates nothing, returns an owned
//!   value and reaches no function without a body other than a pure one, so
//!   that it answers the same in the twin and in the block;
//! - no kept call follows the block's object statement in source order, so
//!   that a call that never returns stops the twin no earlier than it would
//!   stop the block after that statement's effect;
//! - nothing the kept expressions read is written between the twin and the
//!   block's end: a binding they read is never the root of a `set` in the
//!   block, never released or consumed there, has no address taken in the
//!   function, and a reference they read through names only parameters the
//!   function's row does not write [EFF-2].
//!
//! Each condition errs toward no twin. The runtime stops a program whose
//! block reaches a key its twin did not collect, which only a defect here
//! could cause.

use std::collections::{HashMap, HashSet};

use super::model::{
    BindingId, CheckedAtomicForm, CheckedConversionMode, CheckedDrop, CheckedExpression,
    CheckedFunction, CheckedIntegerOperation, CheckedMatchArm, CheckedMode, CheckedRangeSource,
    CheckedSetTarget, CheckedStatement, FunctionId, expression_children,
};
use super::places::PlaceRoot;
use crate::NodePath;

/// [SHARE-3] whether some statement of the program that holds an entry or a
/// map's state runs two statements on shared objects in its block.
///
/// Such a block is a section: while it runs, no statement holding the whole
/// map runs, so none sees the objects between its two statements. A
/// statement that held only its keys' entries would run beside it and could.
/// So no statement of such a program holds less than its map. In a program
/// without one, every statement holding an entry or a map takes effect as if
/// at the one object statement its block runs, what it holds being
/// unchanged by others until it ends, and every outcome is one some order of
/// whole statements gives.
pub(crate) fn runs_object_sections(functions: &[CheckedFunction]) -> bool {
    functions.iter().any(|function| {
        function
            .body
            .as_deref()
            .is_some_and(|body| !sections_are_single(body))
    })
}

/// Whether every block of `statements` that holds an entry or a map's state
/// runs at most one object statement.
fn sections_are_single(statements: &[CheckedStatement]) -> bool {
    statements.iter().all(|statement| match statement {
        CheckedStatement::Atomic {
            form: CheckedAtomicForm::Object,
            ..
        } => true,
        CheckedStatement::Atomic { body, .. } => one_object_statement(body),
        CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
            sections_are_single(body)
        }
        CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
            arms.iter().all(|arm| sections_are_single(&arm.body))
        }
        _ => true,
    })
}

/// The twin of each statement of `function` that holds a map's state and can
/// hold its keys' entries instead, by the statement's node, in a program no
/// statement of which runs an object section ([`runs_object_sections`]).
/// `addressed` is the function's bindings whose storage a borrow or a
/// storage path reaches.
pub(crate) fn key_twins(
    function: &CheckedFunction,
    functions: &[CheckedFunction],
    addressed: &HashSet<BindingId>,
) -> HashMap<NodePath, Vec<CheckedStatement>> {
    let mut twins = HashMap::new();
    if let Some(body) = &function.body {
        let mut repeatable = HashMap::new();
        find(
            function,
            functions,
            addressed,
            body,
            &mut repeatable,
            &mut twins,
        );
    }
    twins
}

fn find(
    function: &CheckedFunction,
    functions: &[CheckedFunction],
    addressed: &HashSet<BindingId>,
    statements: &[CheckedStatement],
    repeatable: &mut HashMap<FunctionId, bool>,
    twins: &mut HashMap<NodePath, Vec<CheckedStatement>>,
) {
    for statement in statements {
        match statement {
            CheckedStatement::Atomic {
                node_path,
                form: CheckedAtomicForm::Map,
                binding,
                body,
                fallthrough_drops,
                ..
            } => {
                let twin = Twin {
                    function,
                    functions,
                    addressed,
                    repeatable: &mut *repeatable,
                    state: *binding,
                    read: HashSet::new(),
                    kept: HashSet::new(),
                    calls: false,
                };
                if let Some(twin) = twin.of(body, fallthrough_drops) {
                    twins.insert(node_path.clone(), twin);
                }
            }
            CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
                for arm in arms {
                    find(function, functions, addressed, &arm.body, repeatable, twins);
                }
            }
            CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
                find(function, functions, addressed, body, repeatable, twins);
            }
            // [SHARE-2] no other atomic statement's block contains a
            // statement holding a map's state.
            _ => {}
        }
    }
}

/// [SHARE-3] whether a block runs at most one statement holding a shared
/// object's state on any path through it: a match runs one of its arms, and
/// a loop that contains such a statement may run it twice.
///
/// A block that runs two such statements reads or writes objects twice, and
/// what it holds exclusively is all that keeps another statement's object
/// statements from falling between the two. So a statement that shares what
/// it holds with statements that only read it, or holds less than the map
/// its source names, does so only when its block runs at most one: the
/// block then takes effect as if at that one statement's point, since what
/// it holds changes for no other statement before it ends.
pub(crate) fn one_object_statement(statements: &[CheckedStatement]) -> bool {
    objects_run(statements) <= 1
}

/// The most statements holding a shared object's state that one run of
/// `statements` executes, counted as two once it may be more than one.
fn objects_run(statements: &[CheckedStatement]) -> usize {
    statements
        .iter()
        .map(|statement| match statement {
            CheckedStatement::Atomic {
                form: CheckedAtomicForm::Object,
                ..
            } => 1,
            CheckedStatement::Atomic { body, .. } => objects_run(body),
            CheckedStatement::Loop { body, .. } | CheckedStatement::CountedRange { body, .. } => {
                if objects_run(body) == 0 {
                    0
                } else {
                    2
                }
            }
            CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
                arms.iter()
                    .map(|arm| objects_run(&arm.body))
                    .max()
                    .unwrap_or(0)
            }
            _ => 0,
        })
        .fold(0, usize::saturating_add)
        .min(2)
}

/// Whether a statement holding a shared object's state lies in `statements`.
fn reaches_object(statements: &[CheckedStatement]) -> bool {
    objects_run(statements) > 0
}

/// Whether `expression` names `binding` anywhere, including in a place root
/// or a captured value: its whole printed form is searched, so that no
/// field of any expression form is missed.
fn mentions(expression: &CheckedExpression, binding: BindingId) -> bool {
    format!("{expression:?}").contains(&format!("BindingId({})", binding.0))
}

/// Whether a statement on an entry of a held state lies anywhere in
/// `statements`.
fn reaches_entry(statements: &[CheckedStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        CheckedStatement::Atomic {
            form: CheckedAtomicForm::Entry { held: true, .. },
            ..
        } => true,
        CheckedStatement::Atomic { body, .. }
        | CheckedStatement::Loop { body, .. }
        | CheckedStatement::CountedRange { body, .. } => reaches_entry(body),
        CheckedStatement::Match { arms, .. } | CheckedStatement::ValueMatchLet { arms, .. } => {
            arms.iter().any(|arm| reaches_entry(&arm.body))
        }
        _ => false,
    })
}

/// Whether an integer operation answers for every operand, with no
/// obligation the checker discharged where it stands.
fn total(operation: CheckedIntegerOperation) -> bool {
    use CheckedIntegerOperation as Op;
    matches!(
        operation,
        Op::AddWrap
            | Op::SubtractWrap
            | Op::MultiplyWrap
            | Op::AbsoluteWrap
            | Op::NegateWrap
            | Op::BitAnd
            | Op::BitOr
            | Op::BitXor
            | Op::BitNot
            | Op::ShiftLeftWrap
            | Op::ShiftRightWrap
            | Op::RotateLeft
            | Op::RotateRight
            | Op::PopulationCount
            | Op::LeadingZeros
            | Op::TrailingZeros
            | Op::ByteSwap
            | Op::MultiplyHigh
            | Op::AddSaturating
            | Op::SubtractSaturating
            | Op::MultiplySaturating
            | Op::Minimum
            | Op::Maximum
            | Op::Equal
            | Op::NotEqual
            | Op::Less
            | Op::LessEqual
            | Op::Greater
            | Op::GreaterEqual
    )
}

/// Every call an expression makes, at any depth.
fn calls_of(expression: &CheckedExpression, calls: &mut Vec<(FunctionId, bool)>) {
    if let CheckedExpression::UserCall {
        function,
        formal_effects,
        formal_contract,
        ..
    } = expression
    {
        calls.push((
            *function,
            formal_effects.is_some() || formal_contract.is_some(),
        ));
    }
    for child in expression_children(expression) {
        calls_of(child, calls);
    }
}

/// Every call the statements make, at any depth; `None` when a statement
/// holds shared state, whose value another context may change.
fn calls_in(statements: &[CheckedStatement], calls: &mut Vec<(FunctionId, bool)>) -> Option<()> {
    for statement in statements {
        match statement {
            CheckedStatement::Let { value, .. }
            | CheckedStatement::DestructuringLet { value, .. }
            | CheckedStatement::Evaluate { value, .. }
            | CheckedStatement::DropExpression { value, .. }
            | CheckedStatement::Return { value, .. }
            | CheckedStatement::Give { value, .. } => calls_of(value, calls),
            CheckedStatement::PropagateLet { scrutinee, .. } => calls_of(scrutinee, calls),
            CheckedStatement::Set { target, value, .. } => {
                match target {
                    CheckedSetTarget::Place(_) => {}
                    CheckedSetTarget::RangeIndex(place) => {
                        for offset in place.offsets() {
                            calls_of(offset, calls);
                        }
                    }
                    CheckedSetTarget::Storage(root) => {
                        for offset in root.offsets() {
                            calls_of(offset, calls);
                        }
                    }
                }
                calls_of(value, calls);
            }
            CheckedStatement::Proof(_) | CheckedStatement::Break { .. } => {}
            CheckedStatement::Match {
                scrutinee, arms, ..
            }
            | CheckedStatement::ValueMatchLet {
                scrutinee, arms, ..
            } => {
                calls_of(scrutinee, calls);
                for arm in arms {
                    calls_in(&arm.body, calls)?;
                }
            }
            CheckedStatement::Loop { body, .. } => calls_in(body, calls)?,
            CheckedStatement::CountedRange {
                lower, upper, body, ..
            } => {
                calls_of(lower, calls);
                calls_of(upper, calls);
                calls_in(body, calls)?;
            }
            CheckedStatement::Atomic { .. } => return None,
        }
    }
    Some(())
}

/// What a block's statements, at any depth, do to bindings.
#[derive(Default)]
struct Facts {
    /// Bindings a statement of the block declares.
    defined: HashSet<BindingId>,
    /// Roots of the block's `set` targets.
    written: HashSet<BindingId>,
    /// Bindings an edge of the block releases.
    released: HashSet<BindingId>,
    /// Bindings an expression of the block consumes.
    consumed: HashSet<BindingId>,
    /// How many value initializers of the block enclose the statement being
    /// examined: a `give` inside one delivers to it, and one outside every
    /// one of them leaves the block.
    initializers: usize,
}

impl Facts {
    /// Collects the facts of `statements`; `None` when an edge leaves the
    /// block or the held state is named other than as an entry's target.
    fn statements(&mut self, statements: &[CheckedStatement], state: BindingId) -> Option<()> {
        for statement in statements {
            match statement {
                CheckedStatement::Let { binding, value, .. } => {
                    self.defined.insert(*binding);
                    self.expression(value, state)?;
                }
                CheckedStatement::DestructuringLet {
                    bindings, value, ..
                } => {
                    self.defined
                        .extend(bindings.iter().map(|(binding, _, _)| *binding));
                    self.expression(value, state)?;
                }
                CheckedStatement::Set { target, value, .. } => {
                    self.written.insert(target.binding());
                    match target {
                        CheckedSetTarget::Place(place) => {
                            if place.declares {
                                self.defined.insert(place.binding);
                            }
                        }
                        CheckedSetTarget::RangeIndex(place) => {
                            for offset in place.offsets() {
                                self.expression(offset, state)?;
                            }
                        }
                        CheckedSetTarget::Storage(root) => {
                            for offset in root.offsets() {
                                self.expression(offset, state)?;
                            }
                        }
                    }
                    self.expression(value, state)?;
                }
                CheckedStatement::Evaluate { value, .. }
                | CheckedStatement::DropExpression { value, .. } => {
                    self.expression(value, state)?;
                }
                CheckedStatement::Proof(_) => {}
                CheckedStatement::Return { .. }
                | CheckedStatement::Break { .. }
                | CheckedStatement::PropagateLet { .. } => return None,
                CheckedStatement::Give { value, drops, .. } => {
                    if self.initializers == 0 {
                        return None;
                    }
                    self.expression(value, state)?;
                    self.drops(drops);
                }
                CheckedStatement::Match {
                    scrutinee, arms, ..
                } => {
                    self.expression(scrutinee, state)?;
                    self.arms(arms, state)?;
                }
                CheckedStatement::ValueMatchLet {
                    binding,
                    scrutinee,
                    arms,
                    ..
                } => {
                    self.defined.insert(*binding);
                    self.expression(scrutinee, state)?;
                    self.initializers += 1;
                    let arms = self.arms(arms, state);
                    self.initializers -= 1;
                    arms?;
                }
                CheckedStatement::Loop {
                    body,
                    backedge_drops,
                    ..
                } => {
                    self.drops(backedge_drops);
                    self.statements(body, state)?;
                }
                CheckedStatement::CountedRange {
                    binder,
                    lower,
                    upper,
                    body,
                    backedge_drops,
                    ..
                } => {
                    self.defined.insert(*binder);
                    self.expression(lower, state)?;
                    self.expression(upper, state)?;
                    self.drops(backedge_drops);
                    self.statements(body, state)?;
                }
                CheckedStatement::Atomic {
                    target,
                    form,
                    key,
                    binding,
                    guard,
                    body,
                    fallthrough_drops,
                    ..
                } => {
                    self.defined.insert(*binding);
                    if matches!(form, CheckedAtomicForm::Entry { held: true, .. }) {
                        // The one place the block may name the held state.
                        if !mentions(target, state) {
                            return None;
                        }
                    } else {
                        self.expression(target, state)?;
                    }
                    for part in key.iter().chain(guard) {
                        self.expression(part, state)?;
                    }
                    self.drops(fallthrough_drops);
                    self.statements(body, state)?;
                }
            }
        }
        Some(())
    }

    fn arms(&mut self, arms: &[CheckedMatchArm], state: BindingId) -> Option<()> {
        for arm in arms {
            self.defined
                .extend(arm.binders.iter().map(|binder| binder.binding));
            self.drops(&arm.fallthrough_drops);
            self.statements(&arm.body, state)?;
        }
        Some(())
    }

    fn drops(&mut self, drops: &[CheckedDrop]) {
        self.released.extend(drops.iter().map(|drop| drop.binding));
    }

    fn expression(&mut self, expression: &CheckedExpression, state: BindingId) -> Option<()> {
        if mentions(expression, state) {
            return None;
        }
        self.consumes(expression);
        Some(())
    }

    fn consumes(&mut self, expression: &CheckedExpression) {
        match expression {
            CheckedExpression::Binding {
                binding,
                consume_root: true,
                ..
            }
            | CheckedExpression::Project {
                binding,
                consume_root: true,
                ..
            }
            | CheckedExpression::BoxTake { binding, .. } => {
                self.consumed.insert(*binding);
            }
            _ => {}
        }
        for child in expression_children(expression) {
            self.consumes(child);
        }
    }
}

struct Twin<'a> {
    function: &'a CheckedFunction,
    functions: &'a [CheckedFunction],
    addressed: &'a HashSet<BindingId>,
    /// Whether each function met so far answers the same when called again
    /// with the same arguments over the same storage.
    repeatable: &'a mut HashMap<FunctionId, bool>,
    /// The binder of the statement holding the map's state.
    state: BindingId,
    /// Bindings the twin's expressions read.
    read: HashSet<BindingId>,
    /// Bindings the twin's own statements define.
    kept: HashSet<BindingId>,
    /// Whether an expression kept so far, which lies later in the block
    /// than the statement now examined, makes a call.
    calls: bool,
}

impl Twin<'_> {
    fn of(
        mut self,
        body: &[CheckedStatement],
        fallthrough_drops: &[CheckedDrop],
    ) -> Option<Vec<CheckedStatement>> {
        if !one_object_statement(body) {
            return None;
        }
        let mut facts = Facts::default();
        // The block's own bindings are released at its end.
        facts.drops(fallthrough_drops);
        facts.statements(body, self.state)?;
        let (twin, reaches) = self.block(body)?;
        if !reaches {
            return None;
        }
        if self
            .kept
            .iter()
            .any(|binding| self.addressed.contains(binding))
        {
            return None;
        }
        for &binding in &self.read {
            if binding == self.state
                || facts.written.contains(&binding)
                || facts.released.contains(&binding)
                || facts.consumed.contains(&binding)
                || !self.names_unwritten_storage(binding)
            {
                return None;
            }
            if self.kept.contains(&binding) {
                continue;
            }
            // Defined in the block by a statement the twin does not keep.
            if facts.defined.contains(&binding) {
                return None;
            }
            let reference = self
                .function
                .parameters
                .iter()
                .find(|parameter| parameter.binding == binding)
                .filter(|parameter| parameter.mode != CheckedMode::Own);
            match reference {
                Some(parameter) => {
                    if self.writes_below(parameter.declaration) {
                        return None;
                    }
                }
                None => {
                    if self.addressed.contains(&binding) {
                        return None;
                    }
                }
            }
        }
        Some(twin)
    }

    /// [EFF-2] whether the function's row writes a path below the parameter
    /// `declaration` declares.
    fn writes_below(&self, declaration: crate::DeclarationId) -> bool {
        self.function
            .declared_state_writes
            .iter()
            .any(|path| path.root == declaration)
    }

    /// Whether every place the reference holder `binding` may name is rooted
    /// at a reference parameter the function's row does not write, or at a
    /// constant; a binding that holds no reference names no place.
    fn names_unwritten_storage(&self, binding: BindingId) -> bool {
        let Some(origins) = self.function.reference_origins.get(binding.0 as usize) else {
            return true;
        };
        origins.iter().all(|place| match place.root {
            PlaceRoot::Constant(_) => true,
            PlaceRoot::Binding(root) => self
                .function
                .parameters
                .iter()
                .find(|parameter| parameter.binding == root)
                .is_some_and(|parameter| {
                    parameter.mode != CheckedMode::Own && !self.writes_below(parameter.declaration)
                }),
        })
    }

    /// Whether a call of `function` answers the same when made again with
    /// the same arguments over storage nothing has written: it waits for
    /// nothing, writes nothing, allocates nothing, calls through no formal
    /// function, holds no shared state, and every function it reaches that
    /// has no body, a host's or the compiler's own, has a pure row. A host
    /// function with a `reads` row may answer from state the host changes,
    /// as the clock's readers do.
    fn repeats(&mut self, function: FunctionId) -> bool {
        if let Some(&known) = self.repeatable.get(&function) {
            return known;
        }
        // A function met again while it is being judged is taken not to
        // repeat, so that no function judged meanwhile is recorded as
        // repeating on the strength of an answer not yet given.
        self.repeatable.insert(function, false);
        let repeats = self.judge(function).is_some();
        self.repeatable.insert(function, repeats);
        repeats
    }

    fn judge(&mut self, function: FunctionId) -> Option<()> {
        let callee = self.functions.get(function.0 as usize)?;
        if callee.waiting.waits || callee.allocates || !callee.declared_state_writes.is_empty() {
            return None;
        }
        let Some(body) = &callee.body else {
            return callee.declared_state_reads.is_empty().then_some(());
        };
        let mut calls = Vec::new();
        calls_in(body, &mut calls)?;
        for (called, formal) in calls {
            if formal || !self.repeats(called) {
                return None;
            }
        }
        Some(())
    }

    /// The twin of `statements`, and whether it reaches an entry. Statements
    /// are taken last first, so that a `let` is met after every expression
    /// that reads its binding.
    fn block(&mut self, statements: &[CheckedStatement]) -> Option<(Vec<CheckedStatement>, bool)> {
        let mut twin = Vec::new();
        let mut reaches = false;
        for statement in statements.iter().rev() {
            match statement {
                CheckedStatement::Atomic {
                    node_path,
                    target,
                    form: form @ CheckedAtomicForm::Entry { held: true, .. },
                    borrowed,
                    key: Some(key),
                    binding,
                    state,
                    body,
                    ..
                } => {
                    // The entry's block runs after its key is computed and
                    // before every expression kept so far.
                    if self.calls && reaches_object(body) {
                        return None;
                    }
                    self.expression(key)?;
                    twin.push(CheckedStatement::Atomic {
                        node_path: node_path.clone(),
                        target: target.clone(),
                        form: *form,
                        borrowed: *borrowed,
                        key: Some(key.clone()),
                        binding: *binding,
                        state: *state,
                        guard: None,
                        body: Vec::new(),
                        fallthrough_drops: Vec::new(),
                        continues: true,
                        invariants: Vec::new(),
                    });
                    reaches = true;
                }
                CheckedStatement::Atomic { .. } => {
                    // A statement on a shared object, which takes effect
                    // before every expression kept so far is evaluated.
                    if self.calls {
                        return None;
                    }
                }
                CheckedStatement::CountedRange {
                    id,
                    node_path,
                    binder,
                    lower,
                    upper,
                    body,
                    ..
                } => {
                    let (inner, inside) = self.block(body)?;
                    if inside {
                        self.expression(lower)?;
                        self.expression(upper)?;
                        self.kept.insert(*binder);
                        twin.push(CheckedStatement::CountedRange {
                            id: *id,
                            node_path: node_path.clone(),
                            binder: *binder,
                            lower: lower.clone(),
                            upper: upper.clone(),
                            invariants: Vec::new(),
                            body: inner,
                            backedge_drops: Vec::new(),
                        });
                        reaches = true;
                    }
                }
                CheckedStatement::Match {
                    scrutinee,
                    enum_type,
                    arms,
                    ..
                } => {
                    let mut bodies = Vec::new();
                    let mut inside = false;
                    for arm in arms {
                        let (body, reached) = self.block(&arm.body)?;
                        inside |= reached;
                        bodies.push(body);
                    }
                    if inside {
                        // A covered payload field is released on entry to its
                        // arm, which the twin must not do.
                        if arms.iter().any(|arm| !arm.covered.is_empty()) {
                            return None;
                        }
                        self.expression(scrutinee)?;
                        twin.push(CheckedStatement::Match {
                            scrutinee: scrutinee.clone(),
                            enum_type: *enum_type,
                            arms: arms
                                .iter()
                                .zip(bodies)
                                .map(|(arm, body)| CheckedMatchArm {
                                    tag: arm.tag,
                                    binders: arm.binders.clone(),
                                    covered: Vec::new(),
                                    body,
                                    fallthrough_drops: Vec::new(),
                                })
                                .collect(),
                            // Whatever the block's arms do, the twin's fall
                            // through to the statements after the match.
                            continues: true,
                        });
                        reaches = true;
                    }
                }
                CheckedStatement::Let { binding, value, .. } if self.read.contains(binding) => {
                    self.expression(value)?;
                    self.kept.insert(*binding);
                    twin.push(statement.clone());
                }
                CheckedStatement::Loop { body, .. }
                    if reaches_entry(body) || (self.calls && reaches_object(body)) =>
                {
                    return None;
                }
                CheckedStatement::ValueMatchLet { arms, .. }
                    if arms.iter().any(|arm| {
                        reaches_entry(&arm.body) || (self.calls && reaches_object(&arm.body))
                    }) =>
                {
                    return None;
                }
                _ => {}
            }
        }
        twin.reverse();
        Some((twin, reaches))
    }

    /// Admits an expression the twin evaluates, recording the bindings it
    /// reads; `None` for a form the twin does not evaluate again.
    fn expression(&mut self, expression: &CheckedExpression) -> Option<()> {
        match expression {
            CheckedExpression::Constant(_) | CheckedExpression::NamedConstant { .. } => {}
            CheckedExpression::Binding {
                binding,
                consume_root: false,
                ..
            } => {
                self.read.insert(*binding);
            }
            CheckedExpression::Project {
                binding,
                consume_root: false,
                residual_drops,
                ..
            } if residual_drops.is_empty() => {
                self.read.insert(*binding);
            }
            CheckedExpression::ProjectValue { value, .. }
            | CheckedExpression::Reinterpret { value, .. }
            | CheckedExpression::NumericConversion {
                mode: CheckedConversionMode::Wrap | CheckedConversionMode::Nearest,
                value,
                ..
            } => self.expression(value)?,
            CheckedExpression::IntegerOperation {
                operation,
                arguments,
                ..
            } if total(*operation) => {
                for argument in arguments {
                    self.expression(argument)?;
                }
            }
            CheckedExpression::FloatOperation { arguments, .. }
            | CheckedExpression::BooleanOperation { arguments, .. }
            | CheckedExpression::EnumEquality { arguments, .. } => {
                for argument in arguments {
                    self.expression(argument)?;
                }
            }
            CheckedExpression::RangeMeasure { root, .. } => {
                self.read.insert(root.binding);
            }
            CheckedExpression::RangeOf {
                source: CheckedRangeSource::Range(root),
                start,
                end,
                ..
            } => {
                self.read.insert(root.binding);
                self.expression(start)?;
                self.expression(end)?;
            }
            // A range over a constant, which nothing writes.
            CheckedExpression::RangeOf {
                source: CheckedRangeSource::Storage(root),
                start,
                end,
                ..
            } if root.binding().is_none() && root.offsets().next().is_none() => {
                self.expression(start)?;
                self.expression(end)?;
            }
            CheckedExpression::UserCall {
                function,
                tail_transfer: false,
                formal_effects: None,
                formal_contract: None,
                arguments,
                requirements,
                result_borrow: None,
                allocation: None,
                ..
            } => {
                let callee = self.functions.get(function.0 as usize)?;
                if callee.result_mode != CheckedMode::Own
                    || !requirements.is_empty()
                    || !self.repeats(*function)
                {
                    return None;
                }
                self.calls = true;
                for argument in arguments {
                    self.expression(argument)?;
                }
            }
            _ => return None,
        }
        Some(())
    }
}
