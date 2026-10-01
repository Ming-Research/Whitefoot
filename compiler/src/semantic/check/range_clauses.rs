//! [RANGE-1, RANGE-4, RANGE-5] the formation of range clauses, of the
//! `use` steps that instantiate them, and of a counted loop's
//! cross-iteration certificate.
//!
//! A range term is formed here from its syntax and resolved names alone. No
//! read below is executed and no subscript owes [OP-4] where it is written:
//! an instance of a range clause claims its relations only where every read
//! it forms selects an existing element, and every user of an instance proves
//! that where it uses one [RANGE-3].

use std::collections::HashMap;

use crate::syntax::NodeId;
use crate::syntax::terminal::{FixedTerminal, TerminalPredicate};
use crate::syntax::views::PlaceSuffix;
use crate::{
    DeclarationClass, DeclarationId, DeclarationRole, LexicalUseRole, Production, ResolvedTarget,
    SemanticCompilerFailure, SemanticIssueKind, SemanticRule,
};

use super::super::model::{
    CheckedMeasure, CheckedMode, CheckedNominalKind, CheckedType, CheckedValue, IntegerType,
};
use super::super::range_facts::{
    CheckedApart, CheckedRangeBinder, CheckedRangeClause, CheckedRangePlace, CheckedRangeRelation,
    CheckedRangeRoot, CheckedRangeShape, CheckedRangeStep, CheckedRangeTerm, CheckedRangeUse,
    RangeComparison,
};
use super::{CheckStop, Checker, FunctionContext, LocalBinding};

/// The names a range term may read besides the function's own bindings.
#[derive(Default)]
struct RangeNames {
    /// The clause's bound variables formed so far, by position.
    binders: HashMap<DeclarationId, u32>,
    /// A certificate's two iterations.
    iterations: HashMap<DeclarationId, u32>,
}

/// What the suffixes of a range place have selected so far.
#[derive(Clone, Copy)]
enum Selected {
    /// A value of this type.
    Value(CheckedType),
    /// A reference binding not yet followed by `^`.
    Holder { mode: CheckedMode, ty: CheckedType },
    /// The run a range reference or a segment names, of this element type.
    Run(CheckedType),
}

impl Checker<'_, '_> {
    /// [RANGE-1] forms one `forall NAME(binders) when guards: conclusions`.
    pub(super) fn check_range_clause(
        &mut self,
        context: FunctionContext<'_, '_>,
        clause: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
    ) -> Result<CheckedRangeClause, CheckStop> {
        let declaration = self
            .types
            .declarations
            .declaration_at(clause, DeclarationRole::RangeFact)?;
        let name = declaration.spelling().to_owned();
        let declaration = declaration.id();
        let mut names = RangeNames::default();
        let mut binders = Vec::new();
        for binder in self
            .types
            .declarations
            .tree
            .children_with(clause, Production::RangeBinder)?
        {
            let endpoints = self
                .types
                .declarations
                .tree
                .children_with(binder, Production::Atom)?;
            let [start, end] = endpoints.as_slice() else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            // A binder's endpoints see the binders before it and never itself.
            let start = self.range_atom(context, *start, bindings, &names)?;
            let end = self.range_atom(context, *end, bindings, &names)?;
            let bound = self
                .types
                .declarations
                .declaration_at(binder, DeclarationRole::RangeBinder)?
                .id();
            let position = u32::try_from(binders.len())
                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
            names.binders.insert(bound, position);
            binders.push(CheckedRangeBinder { start, end });
        }
        if binders.len() > 2 {
            return self.invalid_range(
                SemanticRule::Range1,
                clause,
                "a range clause binds more than two variables",
                "state the fact over at most two bound variables, or split it into facts over fewer",
            );
        }
        let colon = self
            .types
            .declarations
            .tree
            .direct_tokens_matching(clause, &[TerminalPredicate::Fixed(FixedTerminal::Colon)])?;
        let [colon] = colon.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        let colon = u64::try_from(*colon).map_err(|_| SemanticCompilerFailure::CounterOverflow)?;
        let mut guards = Vec::new();
        let mut conclusions = Vec::new();
        for relation in self
            .types
            .declarations
            .tree
            .children_with(clause, Production::RangeRelation)?
        {
            let checked = self.range_relation(context, relation, bindings, &names)?;
            if self
                .types
                .declarations
                .tree
                .first_terminal_position(relation)?
                < colon
            {
                guards.push(checked);
            } else {
                conclusions.push(checked);
            }
        }
        if conclusions.is_empty() {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        Ok(CheckedRangeClause {
            declaration,
            name,
            node: self.types.declarations.tree.path(clause)?.clone(),
            binders,
            guards,
            conclusions,
        })
    }

    /// [RANGE-5, RANGE-4] forms a counted loop's certificate: two iteration
    /// names and the `use` steps instantiating the range facts `facts` names
    /// with each fact's binder count.
    pub(super) fn check_apart_clause(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        facts: &HashMap<DeclarationId, usize>,
    ) -> Result<CheckedApart, CheckStop> {
        let FunctionContext { check_context, .. } = context;
        let iterations = self
            .types
            .declarations
            .declarations_at(node, DeclarationRole::ApartBinder)?;
        let [first, second] = iterations.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidResolution.into());
        };
        if first.spelling() == second.spelling() {
            return self.invalid_range(
                SemanticRule::Range5,
                node,
                "a certificate names its two iterations alike",
                "give the two iterations of `apart` distinct names",
            );
        }
        let mut names = RangeNames::default();
        names.iterations.insert(first.id(), 0);
        names.iterations.insert(second.id(), 1);
        let mut uses = Vec::new();
        for step in self
            .types
            .declarations
            .tree
            .children_with(node, Production::ProofUse)?
        {
            if self
                .types
                .declarations
                .tree
                .has_fixed(step, FixedTerminal::Times)?
            {
                return self.invalid_range(
                    SemanticRule::Range4,
                    step,
                    "a certificate step repeats a premise",
                    "write each instance as its own `use NAME(arguments);` step",
                );
            }
            let premise = self
                .types
                .declarations
                .tree
                .first_child_with(step, Production::UsePremise)?
                .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
            let Some(list) = self
                .types
                .declarations
                .tree
                .first_child_with(premise, Production::AtomList)?
            else {
                return self.invalid_range(
                    SemanticRule::Range4,
                    step,
                    "a certificate step does not instantiate a range fact",
                    "write `use NAME(arguments);` naming a range fact and one term per bound variable",
                );
            };
            let usage = self.types.declarations.use_at(
                check_context,
                premise,
                LexicalUseRole::InvariantFact,
            )?;
            let ResolvedTarget::Source { declaration, .. } = usage.target() else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let Some(arity) = facts.get(&declaration).copied() else {
                return self.invalid_range(
                    SemanticRule::Range4,
                    premise,
                    "the named fact is not a range fact this loop may use",
                    "name a range `requires` of this function or a range invariant of an enclosing counted loop",
                );
            };
            let atoms = self
                .types
                .declarations
                .tree
                .children_with(list, Production::Atom)?;
            if atoms.len() != arity {
                return self.invalid_range(
                    SemanticRule::Range4,
                    premise,
                    "the step gives a different number of terms than the fact has bound variables",
                    "give one term for each bound variable of the fact, in order",
                );
            }
            let mut arguments = Vec::with_capacity(atoms.len());
            for atom in atoms {
                arguments.push(self.range_atom(context, atom, bindings, &names)?);
            }
            uses.push(CheckedRangeUse {
                node: self.types.declarations.tree.path(step)?.clone(),
                fact: declaration,
                arguments,
            });
        }
        Ok(CheckedApart {
            node: self.types.declarations.tree.path(node)?.clone(),
            uses,
        })
    }

    /// The diagnostic one range judgment failure reports [RANGE-3, RANGE-5].
    pub(super) fn range_issue(
        &self,
        issue: &crate::semantic::range_judgment::RangeIssue,
    ) -> CheckStop {
        use crate::semantic::range_judgment::{ApartFailure, RangeIssue};
        let tree = &self.types.declarations.tree;
        // A source spelling with its line, for one access or relation.
        let spell = |path: &crate::NodePath| -> String {
            let text = tree.path_spelling(path).unwrap_or_default();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            match tree.source_line(path) {
                Ok((_, line)) => format!("`{text}` (line {line})"),
                Err(_) => format!("`{text}`"),
            }
        };
        let (path, rule, kind) = match issue {
            RangeIssue::Undischarged {
                node,
                fact,
                site,
                relation,
                capacity,
            } => (
                node,
                SemanticRule::Range3,
                SemanticIssueKind::UndischargedRangeFact {
                    fact: fact.clone(),
                    site,
                    missing: match (relation, capacity) {
                        (_, Some(ceiling)) => format!("the derivation reached {ceiling}"),
                        (Some(relation), None) => spell(relation),
                        (None, None) => {
                            "a place the clause names, which the judgment cannot view here"
                                .to_owned()
                        }
                    },
                    mechanical_fix: if capacity.is_some() {
                        "split the obligation: state fewer facts over the storage this site reads, or move part of the work into a callee with its own range requirements"
                    } else {
                        "establish the fact before this site: a range `requires`, a range invariant of the enclosing counted loop, or a guard that excludes the uncovered elements"
                    },
                },
            ),
            // A checker gap, not a verdict [RANGE-3]: the arithmetic is exact.
            RangeIssue::Arithmetic { node } => {
                return match self.types.declarations.node_location(node) {
                    Ok(location) => CheckStop::Unsupported(crate::semantic::SemanticUnsupported {
                        feature: crate::semantic::UnsupportedSemanticFeature::RangeArithmetic,
                        node: location,
                    }),
                    Err(stop) => stop,
                };
            }
            RangeIssue::Apart { node, failure } => {
                let (pair, mechanical_fix) = match failure {
                    ApartFailure::Overlap {
                        write,
                        other,
                        other_write,
                    } => (
                        format!(
                            "the write {} and the {} {} of another iteration",
                            spell(write),
                            if *other_write { "write" } else { "read" },
                            spell(other)
                        ),
                        "add `use` steps whose instances separate the two accesses, or remove `apart` to leave the loop sequential",
                    ),
                    ApartFailure::Unplaced { access } => (
                        format!(
                            "{} reaches storage another iteration writes, at no one element",
                            spell(access)
                        ),
                        "pass one element or no written storage to the call, or remove `apart` to leave the loop sequential",
                    ),
                    ApartFailure::Use { step, reason } => (
                        format!("the step {}: {reason}", spell(step)),
                        "name a range `requires` or a range invariant of an enclosing loop, with arguments over the loop's own state",
                    ),
                    ApartFailure::Capacity {
                        write,
                        other,
                        ceiling,
                    } => (
                        format!(
                            "the write {} and the access {} reached {ceiling}",
                            spell(write),
                            spell(other)
                        ),
                        "give the certificate fewer accesses to separate: hoist reads that do not depend on the iteration out of the loop",
                    ),
                };
                (
                    node,
                    SemanticRule::Range5,
                    SemanticIssueKind::UndischargedApart {
                        pair,
                        mechanical_fix,
                    },
                )
            }
        };
        match tree.node_with_path(path) {
            Some(node) => self.types.declarations.issue_value(rule, node, kind),
            None => CheckStop::Compiler(SemanticCompilerFailure::InvalidResolution),
        }
    }

    fn invalid_range<Value>(
        &self,
        rule: SemanticRule,
        node: NodeId,
        reason: &'static str,
        mechanical_fix: &'static str,
    ) -> Result<Value, CheckStop> {
        self.types.declarations.issue_node(
            rule,
            node,
            SemanticIssueKind::InvalidRangeClause {
                reason,
                mechanical_fix,
            },
        )
    }

    fn range_relation(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        names: &RangeNames,
    ) -> Result<CheckedRangeRelation, CheckStop> {
        let expressions = self
            .types
            .declarations
            .tree
            .children_with(node, Production::AffineExpr)?;
        let [left, right] = expressions.as_slice() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        let operator = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::CompareOp)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        let [token] = self
            .types
            .declarations
            .tree
            .direct_token_indices(operator)?
        else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        let comparison = match self.types.declarations.tree.token_bytes(*token)? {
            b"==" => RangeComparison::Equal,
            b"!=" => RangeComparison::NotEqual,
            b"<" => RangeComparison::Less,
            b"<=" => RangeComparison::LessEqual,
            b">" => RangeComparison::Greater,
            b">=" => RangeComparison::GreaterEqual,
            _ => return Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        };
        let left = self.range_expression(context, *left, bindings, names)?;
        let right = self.range_expression(context, *right, bindings, names)?;
        Ok(CheckedRangeRelation {
            node: self.types.declarations.tree.path(node)?.clone(),
            left,
            comparison,
            right,
        })
    }

    fn range_expression(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        names: &RangeNames,
    ) -> Result<CheckedRangeTerm, CheckStop> {
        let children = self.types.declarations.tree.children(node)?.to_vec();
        let Some((&first, rest)) = children.split_first() else {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        };
        if rest.len() % 2 != 0 {
            return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
        }
        let mut terms = vec![(1_i128, self.range_product(context, first, bindings, names)?)];
        for pair in rest.as_chunks::<2>().0 {
            let [token] = self.types.declarations.tree.direct_token_indices(pair[0])? else {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            };
            let sign = match self.types.declarations.tree.token_bytes(*token)? {
                b"+" => 1_i128,
                b"-" => -1_i128,
                _ => return Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
            };
            terms.push((sign, self.range_product(context, pair[1], bindings, names)?));
        }
        Ok(range_sum(terms))
    }

    fn range_product(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        names: &RangeNames,
    ) -> Result<CheckedRangeTerm, CheckStop> {
        let factors = self
            .types
            .declarations
            .tree
            .children_with(node, Production::AffineFactor)?;
        match factors.as_slice() {
            [factor] => self.range_factor(context, *factor, bindings, names),
            [left, right] => {
                let left_term = self.range_factor(context, *left, bindings, names)?;
                let right_term = self.range_factor(context, *right, bindings, names)?;
                let (constant, value) = match (&left_term, &right_term) {
                    (CheckedRangeTerm::Constant(constant), _) => (*constant, right_term),
                    (_, CheckedRangeTerm::Constant(constant)) => (*constant, left_term),
                    _ => {
                        return self.invalid_range(
                            SemanticRule::Range1,
                            node,
                            "a range term multiplies two terms neither of which is a constant",
                            "multiply one term by an integer literal or a named integer const",
                        );
                    }
                };
                Ok(range_sum(vec![(constant, value)]))
            }
            _ => Err(SemanticCompilerFailure::InvalidCanonicalTree.into()),
        }
    }

    fn range_factor(
        &mut self,
        context: FunctionContext<'_, '_>,
        node: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        names: &RangeNames,
    ) -> Result<CheckedRangeTerm, CheckStop> {
        if let Some(nested) = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::AffineExpr)?
        {
            return self.range_expression(context, nested, bindings, names);
        }
        if let Some(atom) = self
            .types
            .declarations
            .tree
            .first_child_with(node, Production::Atom)?
        {
            return self.range_atom(context, atom, bindings, names);
        }
        self.invalid_range(
            SemanticRule::Range1,
            node,
            "a range term calls a function",
            "write range terms from literals, consts, integer values, measures and element reads",
        )
    }

    fn range_atom(
        &mut self,
        context: FunctionContext<'_, '_>,
        atom: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        names: &RangeNames,
    ) -> Result<CheckedRangeTerm, CheckStop> {
        if let Some(literal) = self
            .types
            .declarations
            .tree
            .direct_token_with(atom, TerminalPredicate::Literal)?
        {
            let CheckedValue::Integer { ty, bits } =
                self.types.declarations.parse_literal(atom, literal)?
            else {
                return self.invalid_range(
                    SemanticRule::Range1,
                    atom,
                    "a range term is a literal that is not an integer",
                    "write integer literals with a concrete suffix such as `_u64`",
                );
            };
            return Ok(CheckedRangeTerm::Constant(integer_value(ty, bits)));
        }
        if self
            .types
            .declarations
            .tree
            .has_fixed(atom, FixedTerminal::Move)?
        {
            return self.invalid_range(
                SemanticRule::Range1,
                atom,
                "a range term consumes the value it reads",
                "read the value without `move`",
            );
        }
        let Some(place) = self
            .types
            .declarations
            .tree
            .first_child_with(atom, Production::Place)?
        else {
            return self.invalid_range(
                SemanticRule::Range1,
                atom,
                "a range term is a borrow rather than a value",
                "read the value or the element instead of borrowing it",
            );
        };
        self.range_place(context, place, bindings, names)
    }

    fn range_place(
        &mut self,
        context: FunctionContext<'_, '_>,
        place: NodeId,
        bindings: &HashMap<DeclarationId, LocalBinding>,
        names: &RangeNames,
    ) -> Result<CheckedRangeTerm, CheckStop> {
        let FunctionContext { check_context, .. } = context;
        let pbase = self
            .types
            .declarations
            .tree
            .first_child_with(place, Production::Pbase)?
            .ok_or(SemanticCompilerFailure::InvalidCanonicalTree)?;
        if !self.types.declarations.tree.children(pbase)?.is_empty() {
            return self.invalid_range(
                SemanticRule::Range1,
                place,
                "a range term reads an entry image",
                "read the current value; a range clause states one state",
            );
        }
        let suffixes = self
            .types
            .declarations
            .tree
            .children_with(place, Production::Psuffix)?;
        let usage =
            self.types
                .declarations
                .use_at(check_context, pbase, LexicalUseRole::PlaceBase)?;
        let ResolvedTarget::Source { declaration, class } = usage.target() else {
            return self.invalid_range(
                SemanticRule::Range1,
                place,
                "a range term names something that is not a value",
                "name an integer value, a const, a bound variable or a storage place",
            );
        };
        if let Some(position) = names.binders.get(&declaration) {
            if !suffixes.is_empty() {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            }
            return Ok(CheckedRangeTerm::Bound(*position));
        }
        if let Some(position) = names.iterations.get(&declaration) {
            if !suffixes.is_empty() {
                return Err(SemanticCompilerFailure::InvalidCanonicalTree.into());
            }
            return Ok(CheckedRangeTerm::Iteration(*position));
        }
        if class == DeclarationClass::NamedConst && suffixes.is_empty() {
            let Some(constant) = self.types.constants.get(&declaration).copied() else {
                return Err(SemanticCompilerFailure::InvalidResolution.into());
            };
            let constant = self.types.constant(constant)?;
            let CheckedValue::Integer { ty, bits } = &constant.value else {
                return self.invalid_range(
                    SemanticRule::Range1,
                    place,
                    "a range term names a const that is not an integer",
                    "name an integer const",
                );
            };
            return Ok(CheckedRangeTerm::Constant(integer_value(*ty, *bits)));
        }
        if class != DeclarationClass::Value {
            return self.invalid_range(
                SemanticRule::Range1,
                place,
                "a range term names something that is not a value",
                "name an integer value, a const, a bound variable or a storage place",
            );
        }
        let Some(local) = bindings.get(&declaration) else {
            return self.invalid_range(
                SemanticRule::Range1,
                place,
                "a range term names a value that is not live here",
                "name a parameter or a value bound before this clause",
            );
        };
        let mut selected = match local.mode {
            CheckedMode::Own => Selected::Value(local.ty),
            mode => Selected::Holder { mode, ty: local.ty },
        };
        let mut path = Vec::new();
        let last = suffixes.len();
        let mut index = 0;
        while index < last {
            let suffix = suffixes[index];
            let at_end = index + 1 == last;
            match self.types.declarations.tree.place_suffix(suffix)? {
                PlaceSuffix::Dereference => {
                    let Selected::Holder { mode, ty } = selected else {
                        return self.invalid_range(
                            SemanticRule::Range1,
                            suffix,
                            "a range term dereferences a value that is not a reference",
                            "write `^` only after a reference binding",
                        );
                    };
                    path.push(CheckedRangeStep::Referent);
                    selected = if mode == CheckedMode::Range {
                        Selected::Run(ty)
                    } else {
                        Selected::Value(ty)
                    };
                }
                PlaceSuffix::Range { .. } => {
                    return self.invalid_range(
                        SemanticRule::Range1,
                        suffix,
                        "a range term forms a range of elements",
                        "read one element by its index",
                    );
                }
                PlaceSuffix::Index { offset } => {
                    let index_term = self.range_atom(context, offset, bindings, names)?;
                    let root = CheckedRangePlace {
                        root: CheckedRangeRoot::Binding(local.binding),
                        path: path.clone(),
                    };
                    let element = match selected {
                        Selected::Run(element) => element,
                        Selected::Value(CheckedType::Array { element, .. })
                        | Selected::Value(CheckedType::Window { element, .. })
                        | Selected::Value(CheckedType::Buffer { element }) => {
                            self.types.element_type(element)?
                        }
                        Selected::Value(CheckedType::Segments { element }) => {
                            let element = self.types.element_type(element)?;
                            // [RANGE-1] `s[d][k]` reads element k of segment
                            // d, and `s[d].len` is that segment's length.
                            let Some(&next) = suffixes.get(index + 1) else {
                                return self.invalid_range(
                                    SemanticRule::Range1,
                                    suffix,
                                    "a range term selects a segment without reading it",
                                    "read an element `s[d][k]` or the length `s[d].len`",
                                );
                            };
                            if index + 2 != last {
                                return self.invalid_range(
                                    SemanticRule::Range1,
                                    next,
                                    "a range term continues past a segment element",
                                    "end the term at the integer element or the segment length",
                                );
                            }
                            return match self.types.declarations.tree.place_suffix(next)? {
                                PlaceSuffix::Index { offset } => {
                                    let element_index =
                                        self.range_atom(context, offset, bindings, names)?;
                                    let CheckedType::Integer(integer) = element else {
                                        return self.not_integer_element(next);
                                    };
                                    Ok(CheckedRangeTerm::Read {
                                        place: root,
                                        shape: CheckedRangeShape::Segments,
                                        indices: vec![index_term, element_index],
                                        element: integer,
                                    })
                                }
                                PlaceSuffix::Member(_) => {
                                    let name = self.member_name(next)?;
                                    if name != "len" {
                                        return self.invalid_range(
                                            SemanticRule::Range1,
                                            next,
                                            "a range term selects a field of a segment",
                                            "read an element `s[d][k]` or the length `s[d].len`",
                                        );
                                    }
                                    Ok(CheckedRangeTerm::SegmentLength {
                                        place: root,
                                        segment: Box::new(index_term),
                                    })
                                }
                                _ => self.invalid_range(
                                    SemanticRule::Range1,
                                    next,
                                    "a range term dereferences a segment",
                                    "read an element `s[d][k]` or the length `s[d].len`",
                                ),
                            };
                        }
                        _ => {
                            return self.invalid_range(
                                SemanticRule::Range1,
                                suffix,
                                "a range term subscripts a value that is not a run of elements",
                                "subscript an array, a slots window, a range reference's run or a segments value",
                            );
                        }
                    };
                    if !at_end {
                        return self.invalid_range(
                            SemanticRule::Range1,
                            suffix,
                            "a range term selects below an element",
                            "end the term at the integer element it reads",
                        );
                    }
                    let CheckedType::Integer(integer) = element else {
                        return self.not_integer_element(suffix);
                    };
                    return Ok(CheckedRangeTerm::Read {
                        place: root,
                        shape: CheckedRangeShape::Run,
                        indices: vec![index_term],
                        element: integer,
                    });
                }
                PlaceSuffix::Member(_) => {
                    let name = self.member_name(suffix)?;
                    if at_end && (name == "len" || name == "cap") {
                        let measure = if name == "len" {
                            CheckedMeasure::Length
                        } else {
                            CheckedMeasure::Capacity
                        };
                        let measured = match selected {
                            Selected::Run(_) => measure == CheckedMeasure::Length,
                            Selected::Value(ty) => ty.measured().is_some(),
                            Selected::Holder { .. } => false,
                        };
                        if measured {
                            return Ok(CheckedRangeTerm::Measure {
                                place: CheckedRangePlace {
                                    root: CheckedRangeRoot::Binding(local.binding),
                                    path,
                                },
                                measure,
                            });
                        }
                    }
                    let Selected::Value(value) = selected else {
                        return self.invalid_range(
                            SemanticRule::Range1,
                            suffix,
                            "a range term selects a field of a reference or a run",
                            "dereference the reference with `^` first",
                        );
                    };
                    if let Some(content) = self.types.box_content(value)? {
                        if name != "inner" {
                            return self.invalid_range(
                                SemanticRule::Range1,
                                suffix,
                                "a range term selects a field a `Box` does not have",
                                "select a `Box`'s content with `.inner`",
                            );
                        }
                        path.push(CheckedRangeStep::BoxContent);
                        selected = Selected::Value(content);
                    } else {
                        let CheckedType::Nominal(nominal) = value else {
                            return self.invalid_range(
                                SemanticRule::Range1,
                                suffix,
                                "a range term selects a field of a value that has none",
                                "select a field of a struct",
                            );
                        };
                        let CheckedNominalKind::Struct { fields } =
                            &self.types.nominal(nominal)?.kind
                        else {
                            return self.invalid_range(
                                SemanticRule::Range1,
                                suffix,
                                "a range term selects a field of a value that is not a struct",
                                "select a field of a struct",
                            );
                        };
                        let Some((ordinal, field)) = fields
                            .iter()
                            .enumerate()
                            .find(|(_, field)| field.name == name)
                        else {
                            return self.invalid_range(
                                SemanticRule::Range1,
                                suffix,
                                "a range term selects a field the struct does not declare",
                                "select a declared field",
                            );
                        };
                        let ty = field.ty;
                        path.push(CheckedRangeStep::Field(
                            u32::try_from(ordinal)
                                .map_err(|_| SemanticCompilerFailure::CounterOverflow)?,
                        ));
                        selected = Selected::Value(ty);
                    }
                }
            }
            index += 1;
        }
        match selected {
            Selected::Value(CheckedType::Integer(_)) if path.is_empty() => Ok(
                CheckedRangeTerm::Value(CheckedRangeRoot::Binding(local.binding)),
            ),
            _ => self.invalid_range(
                SemanticRule::Range1,
                place,
                "a range term reads a place that is not an integer value, a measure or an element",
                "bind the integer in a `let` before the clause, or read one integer element",
            ),
        }
    }

    fn member_name(&self, suffix: NodeId) -> Result<String, CheckStop> {
        Ok(self
            .types
            .declarations
            .deferred_use_at(suffix, crate::DeferredUseRole::ProjectedField)?
            .spelling()
            .to_owned())
    }

    fn not_integer_element<Value>(&self, node: NodeId) -> Result<Value, CheckStop> {
        self.invalid_range(
            SemanticRule::Range1,
            node,
            "a range term reads an element that is not an integer",
            "read elements of an integer type",
        )
    }
}

/// The canonical sum of weighted terms: nested sums flatten, constants fold,
/// and a single unit term stands alone.
fn range_sum(terms: Vec<(i128, CheckedRangeTerm)>) -> CheckedRangeTerm {
    let mut constant = 0_i128;
    let mut flat: Vec<(i128, CheckedRangeTerm)> = Vec::new();
    let mut pending = terms;
    while let Some((weight, term)) = pending.pop() {
        match term {
            CheckedRangeTerm::Constant(value) => {
                constant = constant.saturating_add(weight.saturating_mul(value));
            }
            CheckedRangeTerm::Sum {
                terms,
                constant: inner,
            } => {
                constant = constant.saturating_add(weight.saturating_mul(inner));
                for (coefficient, nested) in terms {
                    pending.push((weight.saturating_mul(coefficient), nested));
                }
            }
            other => {
                if let Some(existing) = flat.iter_mut().find(|(_, term)| *term == other) {
                    existing.0 = existing.0.saturating_add(weight);
                } else {
                    flat.push((weight, other));
                }
            }
        }
    }
    flat.retain(|(weight, _)| *weight != 0);
    flat.sort_by(|left, right| left.1.cmp(&right.1));
    if constant == 0 && flat.len() == 1 && flat[0].0 == 1 {
        return flat
            .pop()
            .map(|(_, term)| term)
            .unwrap_or(CheckedRangeTerm::Constant(0));
    }
    if flat.is_empty() {
        return CheckedRangeTerm::Constant(constant);
    }
    CheckedRangeTerm::Sum {
        terms: flat,
        constant,
    }
}

const fn integer_value(ty: IntegerType, bits: u64) -> i128 {
    let value = bits as i128;
    if ty.signed() {
        let width = ty.width() as u32;
        let sign_bit = 1_u64 << (width - 1);
        if bits & sign_bit != 0 {
            return value - (1_i128 << width);
        }
    }
    value
}
