//! [MOD-8, FN-9] proof receipts: the retained outcome of one function's
//! verified entailment analysis, reused by a later check that would analyze
//! the same checked function against the same boundaries.
//!
//! A function's analysis reads its own checked body and, of every other
//! entity, only what the entailment context lends it: a callee's projection,
//! requirements and published postconditions, a nominal's or element's
//! definition, a constant's value, a contract query's outcome. The key of a
//! receipt is the canonical text of exactly that: the function's derived
//! `Debug` rendering and the rendering of every entity it names, closed under
//! naming. Program-wide dense identities are spelled by stable identity
//! (a symbol, the key resolution minted for a declaration or an item, a
//! stable type spelling, a node's key relative to its item), and
//! declarations local to a function are numbered by first appearance, so
//! adding an unrelated declaration, reordering items or moving a record
//! renames nothing a key reads. A rendering that names an identity no spelling covers yields no
//! key, and the function is analyzed as if no receipt existed.
//!
//! A receipt records only what an accepted analysis concluded and later
//! stages read: that every judgment held, which postconditions it published
//! to callers, the body's disposition, and each proved allocation ceiling.
//! A rejection is never recorded, so a failing function is analyzed afresh
//! and reported where its current source stands.

use std::collections::{HashMap, HashSet};

use super::{CheckStop, CheckedFunctionInventory, Checker};
use crate::semantic::entailment::{
    DerivationId, EntailmentCallee, FunctionEntailment, FunctionPostconditionProof,
    ObligationFamily, ObligationOutcome, PostconditionAggregate,
};
use crate::semantic::model::{
    CheckedBodyDisposition, CheckedContractQuery, CheckedFunction, CheckedType, IntegerType,
    NominalId,
};
use crate::semantic::postcondition::CheckedPostcondition;
use crate::{DeclarationId, DeclarationRole, NodePath};

/// The identifier kinds a checked value's rendering can carry, by the
/// newtype names their derived `Debug` prints.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) enum IdKind {
    Function,
    Nominal,
    Declaration,
    Constant,
    Element,
    ContractQuery,
    DerivedConst,
    Module,
    FunctionReference,
    Node,
    Source,
    Scope,
    Offset,
}

impl IdKind {
    /// The kind a newtype name denotes, when it is a program-wide identity.
    fn program_wide(name: &str) -> Option<Self> {
        Some(match name {
            "FunctionId" => Self::Function,
            "NominalId" => Self::Nominal,
            "DeclarationId" => Self::Declaration,
            "CheckedConstantId" => Self::Constant,
            "CheckedElement" => Self::Element,
            "ContractQueryId" => Self::ContractQuery,
            "DerivedConstId" => Self::DerivedConst,
            "ModuleId" => Self::Module,
            "FunctionReferenceId" => Self::FunctionReference,
            "NodeId" => Self::Node,
            "SourceId" => Self::Source,
            "ScopeId" => Self::Scope,
            "ByteOffset" => Self::Offset,
            _ => return None,
        })
    }
}

/// Newtypes whose values are local to one function's analysis or fixed by
/// the compiler itself, so their dense value is already stable.
fn stable_by_construction(name: &str) -> bool {
    matches!(
        name,
        "BindingId"
            | "CheckedLoopId"
            | "TermId"
            | "GoalId"
            | "DerivationId"
            | "FlowEventId"
            | "AffineTermId"
            | "BuiltinPreludeId"
            | "ContainerNominalId"
            | "OperationFamilyId"
            | "PreludeDeclarationId"
            | "GrammarNodeId"
    )
}

/// How one program-wide identity is spelled in a key.
pub(super) enum Spelling {
    /// By a stable identity.
    Stable(String),
    /// A declaration local to a function: numbered by first appearance in
    /// the key.
    Local,
    /// Numbered by first appearance like a local, and expanded where first
    /// named, like a stable identity: an identity with no name of its own.
    Numbered,
}

/// The canonical text of one receipt key under construction.
#[derive(Default)]
pub(super) struct StableText {
    text: String,
    locals: HashMap<(IdKind, u64), usize>,
    named: Vec<(IdKind, u64)>,
    seen: HashSet<(IdKind, u64)>,
    /// The spelling each named identity took, and the identities that took
    /// each spelling, so two identities one spelling covers stay apart.
    spelled: HashMap<(IdKind, u64), String>,
    spellers: HashMap<String, Vec<(IdKind, u64)>>,
}

impl StableText {
    /// Appends a section label, which keeps the renderings of different
    /// entities apart.
    pub(super) fn label(&mut self, label: &str) {
        self.text.push('\n');
        self.text.push_str(label);
        self.text.push('\n');
    }

    /// Appends one derived `Debug` rendering with every program-wide
    /// identity respelled: `spell` spells a dense identity, and `item` spells
    /// the top-level item a node path's first component selects. Returns
    /// `None`, leaving the text unusable, when the rendering names an
    /// identity neither can spell.
    pub(super) fn push(
        &mut self,
        rendering: &str,
        spell: &mut dyn FnMut(IdKind, u64) -> Option<Spelling>,
        item: &mut dyn FnMut(u32) -> Option<String>,
    ) -> Option<()> {
        let bytes = rendering.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            let byte = bytes[index];
            if byte == b'"' {
                // A string literal is data: copied as written, escapes and all.
                let start = index;
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index += 2,
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
                let end = index.min(bytes.len());
                self.text.push_str(rendering.get(start..end)?);
                continue;
            }
            if byte.is_ascii_alphabetic() || byte == b'_' {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                let word = rendering.get(start..index)?;
                // A source coordinate locates a diagnostic and decides
                // nothing an accepted analysis concludes; its byte offsets
                // move with every edit earlier in its file.
                if word == "SyntaxCoordinate"
                    && let Some(end) = coordinate(bytes, index)
                {
                    self.text.push_str("SyntaxCoordinate");
                    index = end;
                    continue;
                }
                if word == "NodePath"
                    && let Some((components, end)) = node_path(bytes, index)
                {
                    self.text.push_str("NodePath(");
                    if let Some((first, rest)) = components.split_first() {
                        self.text.push_str(&item(*first)?);
                        for component in rest {
                            self.text.push('/');
                            self.text.push_str(&component.to_string());
                        }
                    } else {
                        self.text.push_str("root");
                    }
                    self.text.push(')');
                    index = end;
                    continue;
                }
                if let Some((value, end)) = parenthesized_number(bytes, index) {
                    // `CaptureId::Source` carries the node id of its source
                    // occurrence; no other checked value writes a bare
                    // number after `Source`.
                    let kind = if word == "Source" {
                        Some(IdKind::Node)
                    } else {
                        IdKind::program_wide(word)
                    };
                    if let Some(kind) = kind {
                        self.text.push_str(word);
                        self.text.push('(');
                        match spell(kind, value)? {
                            Spelling::Stable(spelling) => {
                                let spelling = self.distinct(kind, value, spelling);
                                self.text.push_str(&spelling);
                                if self.seen.insert((kind, value)) {
                                    self.named.push((kind, value));
                                }
                            }
                            Spelling::Local => {
                                let next = self.locals.len();
                                let ordinal = *self.locals.entry((kind, value)).or_insert(next);
                                self.text.push_str("local ");
                                self.text.push_str(&ordinal.to_string());
                            }
                            Spelling::Numbered => {
                                let next = self.locals.len();
                                let ordinal = *self.locals.entry((kind, value)).or_insert(next);
                                self.text.push_str("numbered ");
                                self.text.push_str(&ordinal.to_string());
                                if self.seen.insert((kind, value)) {
                                    self.named.push((kind, value));
                                }
                            }
                        }
                        self.text.push(')');
                        index = end;
                        continue;
                    }
                    // A dense identity this key has no spelling for: refuse
                    // rather than read an order-dependent number as identity.
                    if !stable_by_construction(word) && word.ends_with("Id") {
                        return None;
                    }
                }
                self.text.push_str(word);
                continue;
            }
            let character = rendering.get(index..)?.chars().next()?;
            self.text.push(character);
            index += character.len_utf8();
        }
        Some(())
    }

    /// The spelling of one identity in this key: its stable spelling, with
    /// the order of first appearance among the identities that share it
    /// appended when another took it first, so the key keeps which
    /// occurrence names which identity even where a stable spelling leaves
    /// out a distinction, such as a `Box` type's region.
    fn distinct(&mut self, kind: IdKind, value: u64, spelling: String) -> String {
        if let Some(spelled) = self.spelled.get(&(kind, value)) {
            return spelled.clone();
        }
        let sharers = self.spellers.entry(spelling.clone()).or_default();
        let distinct = if sharers.is_empty() {
            spelling
        } else {
            format!("{spelling}#{}", sharers.len())
        };
        sharers.push((kind, value));
        self.spelled.insert((kind, value), distinct.clone());
        distinct
    }

    /// The program-wide identities named so far, in first-appearance order,
    /// from `from` on.
    pub(super) fn named_since(&self, from: usize) -> &[(IdKind, u64)] {
        self.named.get(from..).unwrap_or_default()
    }

    /// The local declarations the key numbered, with their ordinals.
    pub(super) fn local_declarations(&self) -> Vec<(u64, usize)> {
        self.locals
            .iter()
            .filter(|((kind, _), _)| *kind == IdKind::Declaration)
            .map(|((_, value), ordinal)| (*value, *ordinal))
            .collect()
    }

    /// The finished key text.
    pub(super) fn into_text(self) -> String {
        self.text
    }
}

/// `(digits)` at `index`: the number and the index after the parenthesis.
fn parenthesized_number(bytes: &[u8], index: usize) -> Option<(u64, usize)> {
    let rest = bytes.get(index..)?;
    let rest = rest.strip_prefix(b"(")?;
    let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    if digits == 0 || rest.get(digits) != Some(&b')') {
        return None;
    }
    let value = std::str::from_utf8(rest.get(..digits)?)
        .ok()?
        .parse()
        .ok()?;
    Some((value, index + 1 + digits + 1))
}

/// ` { source: SourceId(a), start: ByteOffset(b), end: ByteOffset(c) }` at
/// `index`: the index after the closing brace.
fn coordinate(bytes: &[u8], index: usize) -> Option<usize> {
    let mut at = index;
    for (label, name) in [
        (&b" { source: "[..], &b"SourceId"[..]),
        (&b", start: "[..], &b"ByteOffset"[..]),
        (&b", end: "[..], &b"ByteOffset"[..]),
    ] {
        at += label.len();
        if bytes.get(at - label.len()..at)? != label {
            return None;
        }
        at += name.len();
        if bytes.get(at - name.len()..at)? != name {
            return None;
        }
        let (_, end) = parenthesized_number(bytes, at)?;
        at = end;
    }
    (bytes.get(at..at + 2)? == b" }").then_some(at + 2)
}

/// ` { components: [a, b, c] }` at `index`: the components and the index
/// after the closing brace.
fn node_path(bytes: &[u8], index: usize) -> Option<(Vec<u32>, usize)> {
    let rest = bytes.get(index..)?;
    let prefix = b" { components: [";
    let rest = rest.strip_prefix(prefix.as_slice())?;
    let close = rest.iter().position(|byte| *byte == b']')?;
    let list = std::str::from_utf8(rest.get(..close)?).ok()?;
    let components = if list.trim().is_empty() {
        Vec::new()
    } else {
        list.split(", ")
            .map(|component| component.parse().ok())
            .collect::<Option<Vec<u32>>>()?
    };
    let after = rest.get(close..)?.strip_prefix(b"] }")?;
    Some((components, bytes.len() - after.len()))
}

/// Where a check finds and keeps proof receipts [MOD-8].
///
/// A store returns only what it was given for exactly the same key; the
/// driver's implementation is the build cache, whose records are checked
/// byte for byte against their key.
pub(crate) trait ProofReceipts {
    /// The receipt recorded for exactly `key`, when one is present.
    fn load(&self, key: &[u8]) -> Option<Vec<u8>>;
    /// Records the receipt of `key`.
    fn store(&self, key: &[u8], receipt: &[u8]);
}

/// What one verified analysis concluded that anything after it reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProofReceipt {
    /// The body's disposition at entry [ENT-5]: an uninhabited body
    /// publishes nothing and lowers to an unreachable terminator.
    pub(super) uninhabited: bool,
    /// Each proved [OP-9] allocation ceiling, by the spelled path of its
    /// allocation site.
    pub(super) allocation_bounds: Vec<(String, u64)>,
}

impl ProofReceipt {
    const HEADER: &'static str = "proof-receipt 1";

    pub(super) fn encode(&self) -> Vec<u8> {
        let mut text = format!(
            "{}\n{}\n",
            Self::HEADER,
            if self.uninhabited {
                "uninhabited"
            } else {
                "inhabited"
            }
        );
        for (site, bound) in &self.allocation_bounds {
            text.push_str(&format!("bound {bound} {site}\n"));
        }
        text.into_bytes()
    }

    pub(super) fn decode(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes).ok()?;
        let mut lines = text.lines();
        if lines.next()? != Self::HEADER {
            return None;
        }
        let uninhabited = match lines.next()? {
            "inhabited" => false,
            "uninhabited" => true,
            _ => return None,
        };
        let mut allocation_bounds = Vec::new();
        for line in lines {
            let rest = line.strip_prefix("bound ")?;
            let (bound, site) = rest.split_once(' ')?;
            allocation_bounds.push((site.to_owned(), bound.parse().ok()?));
        }
        Some(Self {
            uninhabited,
            allocation_bounds,
        })
    }
}

/// An item's key as a receipt spells it: a function's interface
/// declaration and its definition are two items of one function, and a
/// receipt spells them alike, since one of them stands for the function in
/// any one check and a receipt recorded by one check is read by another.
fn receipt_item(key: &crate::ItemKey) -> crate::ItemKey {
    match key {
        crate::ItemKey::Declared {
            home: crate::ItemHome::Module { package, path, .. },
            role,
            spelling,
        } => crate::ItemKey::Declared {
            home: crate::ItemHome::Module {
                package: *package,
                path: path.clone(),
                record: crate::SourceRole::Implementation,
            },
            role: *role,
            spelling: spelling.clone(),
        },
        other => other.clone(),
    }
}

/// Stable identities of a checked unit's top-level items, by root-child
/// ordinal, as [`receipt_item`] spells them.
pub(super) struct ItemSpellings {
    by_ordinal: Vec<Option<String>>,
}

impl ItemSpellings {
    fn spelling(&self, ordinal: u32) -> Option<String> {
        self.by_ordinal.get(ordinal as usize)?.clone()
    }
}

/// The rendering of everything one function's analysis reads of the
/// function itself [ENT-5]. The [EFF-3] allocation bit is left out: the
/// analysis never reads it, and it can differ between a module's own check
/// and a composition that sees an allocating callee's body. The obligation
/// records are left out too: they are formed from the requirement places,
/// body, separations and postconditions rendered here, and a receipt stands
/// for an analysis that discharged every one of them.
fn analyzed_rendering(function: &CheckedFunction) -> String {
    let CheckedFunction {
        formal_hypothesis,
        id,
        declaration,
        module,
        name,
        symbol,
        function_actuals,
        region_parameters,
        parameters,
        result_mode,
        result,
        declared_state_writes,
        requirements,
        requirement_places,
        postconditions,
        body,
        reference_origins,
        body_disposition,
        allocates: _,
        call_separations,
        permission_separation_queries,
        obligations: _,
        entailment: _,
    } = function;
    format!(
        "{formal_hypothesis:?}\n{id:?}\n{declaration:?}\n{module:?}\n{name:?}\n{symbol:?}\n\
         {function_actuals:?}\n{region_parameters:?}\n{parameters:?}\n{result_mode:?}\n\
         {result:?}\n{declared_state_writes:?}\n{requirements:?}\n{requirement_places:?}\n\
         {postconditions:?}\n{body:?}\n{reference_origins:?}\n{body_disposition:?}\n\
         {call_separations:?}\n{permission_separation_queries:?}"
    )
}

/// The rendering of what a caller's analysis reads of one callee's written
/// boundary [FN-8, FN-9]: its signature, requirements and each
/// postcondition's selector, substitutions and relation. The occurrences a
/// postcondition selects in the callee's body are the callee's own proof and
/// no caller reads them, so an edit to the callee's body that keeps its
/// boundary leaves every caller's key unchanged.
fn claims_rendering(function: &CheckedFunction) -> String {
    let postconditions = function
        .postconditions
        .iter()
        .map(|postcondition| {
            (
                &postcondition.selector,
                &postcondition.type_substitutions,
                &postcondition.const_substitutions,
                &postcondition.relation,
            )
        })
        .collect::<Vec<_>>();
    format!(
        "{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{postconditions:?}\n{:?}",
        function.formal_hypothesis,
        function.symbol,
        function.module,
        function.parameters,
        function.result_mode,
        function.result,
        function.declared_state_writes,
        function.requirements,
        function.function_actuals,
    )
}

impl Checker<'_> {
    /// The key resolution minted for every top-level item of this unit, by
    /// the item's ordinal among the root's children, as [`receipt_item`]
    /// spells it. An alias binds names only and no analysis reads a node of
    /// one, so an alias item has no spelling here.
    pub(super) fn receipt_items(&self) -> Result<ItemSpellings, CheckStop> {
        let count = self.tree.children(self.tree.root())?.len();
        let by_ordinal = (0..count)
            .map(|ordinal| {
                u32::try_from(ordinal)
                    .ok()
                    .and_then(|ordinal| self.resolved.item_key(ordinal))
                    .filter(|key| !matches!(key, crate::ItemKey::Alias { .. }))
                    .map(|key| receipt_item(key).to_string())
            })
            .collect();
        Ok(ItemSpellings { by_ordinal })
    }

    /// How a receipt key spells one program-wide identity, or `None` when
    /// no stable spelling covers it.
    fn receipt_spelling(&self, kind: IdKind, value: u64) -> Option<Spelling> {
        let index = usize::try_from(value).ok()?;
        match kind {
            IdKind::Function => Some(Spelling::Stable(self.signatures.get(index)?.symbol.clone())),
            IdKind::Nominal => {
                let nominal = NominalId(u32::try_from(index).ok()?);
                self.receipt_type(CheckedType::Nominal(nominal))
                    .map(Spelling::Stable)
            }
            IdKind::Element => {
                let ty = self.elements.borrow().get(index).copied()?;
                self.receipt_type(ty)
                    .map(|spelled| Spelling::Stable(format!("element {spelled}")))
            }
            IdKind::Declaration => {
                let record = self
                    .resolved
                    .declaration(DeclarationId::from_index(index)?)?;
                match record.role() {
                    DeclarationRole::Function
                    | DeclarationRole::Struct
                    | DeclarationRole::Enum
                    | DeclarationRole::Variant
                    | DeclarationRole::Interface
                    | DeclarationRole::Binding
                    | DeclarationRole::NamedConst => Some(Spelling::Stable(match record.key() {
                        crate::DeclarationKey::Item(item) => receipt_item(item).to_string(),
                        crate::DeclarationKey::Local {
                            item,
                            path,
                            ordinal,
                        } => crate::DeclarationKey::Local {
                            item: receipt_item(item),
                            path: path.clone(),
                            ordinal: *ordinal,
                        }
                        .to_string(),
                    })),
                    // An alias binds names only; every use names its target.
                    DeclarationRole::Alias => None,
                    _ => Some(Spelling::Local),
                }
            }
            IdKind::Constant => {
                let declaration = self.checked_constants.get(index)?.declaration;
                match self.receipt_spelling(IdKind::Declaration, declaration.index() as u64)? {
                    Spelling::Stable(spelled) => Some(Spelling::Stable(format!("const {spelled}"))),
                    Spelling::Local | Spelling::Numbered => None,
                }
            }
            // A query is numbered where the key first names it and expanded
            // there, so the key keeps which call uses which query.
            IdKind::ContractQuery => {
                (index < self.contract_queries.borrow().len()).then_some(Spelling::Numbered)
            }
            IdKind::Module => self
                .resolved
                .syntax()
                .classified_bundle()
                .source_bundle()
                .module(crate::ModuleId::from_index(index)?)
                .map(|module| Spelling::Stable(module.qualified_name())),
            // A node id is the occurrence identity of a captured value; its
            // path within its item is stable where the dense id is not.
            IdKind::Node => {
                let node = crate::syntax::NodeId::from_index(index)?;
                let path = self.tree.path(node).ok()?;
                self.resolved
                    .occurrence_key(path)
                    .filter(|key| !matches!(key.item, crate::ItemKey::Alias { .. }))
                    .map(|key| {
                        Spelling::Stable(format!(
                            "node {}",
                            crate::OccurrenceKey {
                                item: receipt_item(&key.item),
                                path: key.path,
                            }
                        ))
                    })
            }
            IdKind::DerivedConst
            | IdKind::FunctionReference
            | IdKind::Source
            | IdKind::Scope
            | IdKind::Offset => None,
        }
    }

    /// The stable spelling of one checked type [FN-2].
    fn receipt_type(&self, ty: CheckedType) -> Option<String> {
        self.stable_type_spelling(ty)
    }

    /// The key under which the analysis of `functions[index]` is recorded,
    /// or `None` when a rendering it reads names an identity no stable
    /// spelling covers. `published` holds, per function, the postconditions
    /// this analysis may use [FN-9].
    pub(super) fn proof_receipt_key(
        &self,
        functions: &[CheckedFunctionInventory],
        index: usize,
        callees: &[EntailmentCallee],
        published: &[Vec<&CheckedPostcondition>],
        const_parameter_types: &HashMap<DeclarationId, IntegerType>,
        items: &ItemSpellings,
    ) -> Option<Vec<u8>> {
        let checked = functions.get(index)?;
        let mut text = StableText::default();
        let mut spell = |kind, value| self.receipt_spelling(kind, value);
        let mut item = |ordinal| items.spelling(ordinal);
        text.label("function");
        text.push(
            &analyzed_rendering(&checked.function),
            &mut spell,
            &mut item,
        )?;
        text.label("bindings");
        text.push(
            &format!("{:?}", checked.binding_names),
            &mut spell,
            &mut item,
        )?;
        // Close over every identity the key names, in first-appearance order;
        // an expansion may name more.
        let mut expanded = 0;
        while let Some(&(kind, value)) = text.named_since(expanded).first() {
            expanded += 1;
            let position = usize::try_from(value).ok()?;
            let rendering = match kind {
                IdKind::Function if position != index => {
                    let callee = functions.get(position)?;
                    format!(
                        "callee\n{}\n{:?}\npublished {}",
                        claims_rendering(&callee.function),
                        callees.get(position)?,
                        published.get(position).map_or(0, Vec::len)
                    )
                }
                IdKind::Nominal => {
                    let nominal = self.nominals.get(position)?;
                    format!(
                        "nominal\n{:?}\n{:?} {:?}",
                        nominal.kind, nominal.linear, nominal.nocopy
                    )
                }
                IdKind::Element => {
                    format!("element\n{:?}", self.elements.borrow().get(position)?)
                }
                IdKind::Constant => self.receipt_constant(position)?,
                IdKind::Declaration => {
                    match self.constants.get(&DeclarationId::from_index(position)?) {
                        Some(constant) => self.receipt_constant(constant.0 as usize)?,
                        None => continue,
                    }
                }
                // A query's site records which call first asked it, which the
                // analysis never reads; an instance several modules request
                // is one analysis whichever request came first [FN-2].
                IdKind::ContractQuery => {
                    let queries = self.contract_queries.borrow();
                    let CheckedContractQuery {
                        instance,
                        site: _,
                        premises,
                        goal,
                        proof,
                    } = queries.get(position)?;
                    format!("query\n{instance:?}\n{premises:?}\n{goal:?}\n{proof:?}")
                }
                _ => continue,
            };
            text.label(&format!("expansion {expanded}"));
            text.push(&rendering, &mut spell, &mut item)?;
        }
        // The written integer type of each const parameter the key names
        // [MSR-6], which the analysis reads beside the parameter itself.
        let mut parameters = text
            .local_declarations()
            .into_iter()
            .filter_map(|(declaration, ordinal)| {
                let declaration = DeclarationId::from_index(usize::try_from(declaration).ok()?)?;
                const_parameter_types
                    .get(&declaration)
                    .map(|ty| (ordinal, *ty))
            })
            .collect::<Vec<_>>();
        parameters.sort_by_key(|(ordinal, _)| *ordinal);
        text.label("const parameters");
        text.push(&format!("{parameters:?}"), &mut spell, &mut item)?;
        Some(text.into_text().into_bytes())
    }

    fn receipt_constant(&self, position: usize) -> Option<String> {
        let constant = self.checked_constants.get(position)?;
        Some(format!(
            "constant\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
            constant.declaration,
            constant.name,
            constant.declared_type,
            constant.ty,
            constant.value
        ))
    }

    /// The analysis a recorded receipt stands for, when the store holds one
    /// for this function's key; otherwise `None`, and the key is kept so an
    /// accepted fresh analysis can be recorded under it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn recorded_analysis(
        &self,
        store: &dyn ProofReceipts,
        items: &ItemSpellings,
        functions: &[CheckedFunctionInventory],
        index: usize,
        callees: &[EntailmentCallee],
        published: &[Vec<&CheckedPostcondition>],
        const_parameter_types: &HashMap<DeclarationId, IntegerType>,
    ) -> Option<FunctionEntailment> {
        let key = self.proof_receipt_key(
            functions,
            index,
            callees,
            published,
            const_parameter_types,
            items,
        )?;
        let recorded = store
            .load(&key)
            .and_then(|bytes| ProofReceipt::decode(&bytes))
            .and_then(|receipt| self.receipt_entailment(&functions.get(index)?.function, &receipt));
        if recorded.is_some() {
            if let Some(slot) = self.reused_analyses.borrow_mut().get_mut(index) {
                *slot = true;
            }
        } else {
            self.receipt_keys.borrow_mut().push((index, key));
        }
        recorded
    }

    /// Whether the concrete function at `index` took its analysis from a
    /// receipt.
    pub(super) fn analysis_reused(&self, index: usize) -> bool {
        self.reused_analyses
            .borrow()
            .get(index)
            .copied()
            .unwrap_or(false)
    }

    /// Records the receipt of every function analyzed afresh whose analysis
    /// no rejection names.
    pub(super) fn record_receipts(&self, functions: &[&CheckedFunction], rejected: &[bool]) {
        let Some(store) = self.receipts else {
            return;
        };
        for (index, key) in self.receipt_keys.take() {
            if rejected.get(index).copied().unwrap_or(true) {
                continue;
            }
            if let Some(receipt) = functions
                .get(index)
                .and_then(|function| self.proof_receipt(function))
            {
                store.store(&key, &receipt.encode());
            }
        }
    }

    /// The receipt of an accepted analysis of `function`, or `None` when an
    /// allocation site it proved lies outside its own item.
    pub(super) fn proof_receipt(&self, function: &CheckedFunction) -> Option<ProofReceipt> {
        let own = self.receipt_item_ordinal(function)?;
        let mut allocation_bounds = Vec::new();
        for outcome in &function.entailment.obligations {
            if outcome.family != ObligationFamily::AllocationFit || !outcome.discharged {
                continue;
            }
            let bound = outcome.allocation_length_upper_bound?;
            let (first, rest) = outcome.node_path.components().split_first()?;
            if *first != own {
                return None;
            }
            let site = rest
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("/");
            allocation_bounds.push((site, bound));
        }
        Some(ProofReceipt {
            uninhabited: matches!(
                function.entailment.body_disposition,
                CheckedBodyDisposition::Uninhabited { .. }
            ),
            allocation_bounds,
        })
    }

    /// The root-child ordinal of the item whose body `function` checks.
    fn receipt_item_ordinal(&self, function: &CheckedFunction) -> Option<u32> {
        self.resolved
            .declaration(function.declaration)?
            .origin()
            .node()
            .components()
            .first()
            .copied()
    }

    /// The analysis a receipt stands for: everything later stages read of
    /// it, and every postcondition verified for publication [FN-9].
    pub(super) fn receipt_entailment(
        &self,
        function: &CheckedFunction,
        receipt: &ProofReceipt,
    ) -> Option<FunctionEntailment> {
        let own = self.receipt_item_ordinal(function)?;
        let mut obligations = Vec::with_capacity(receipt.allocation_bounds.len());
        for (site, bound) in &receipt.allocation_bounds {
            let mut components = vec![own];
            if !site.is_empty() {
                for component in site.split('/') {
                    components.push(component.parse().ok()?);
                }
            }
            obligations.push(ObligationOutcome {
                node_path: NodePath { components },
                family: ObligationFamily::AllocationFit,
                conjunct: 0,
                canonical_goal: None,
                components: Vec::new(),
                discharged: true,
                refuted: false,
                contradictory: false,
                residual: None,
                overlap_targets: None,
                derivation: None,
                allocation_length_upper_bound: Some(*bound),
                allocation_length_upper_bound_derivation: None,
                affine_index_maps: Vec::new(),
                range_partitions: Vec::new(),
                written_before: Vec::new(),
            });
        }
        let postconditions = function
            .postconditions
            .iter()
            .enumerate()
            .map(|(ordinal, postcondition)| {
                Some(FunctionPostconditionProof {
                    block: postcondition.selector.block.clone(),
                    selector: postcondition.selector.selector.clone(),
                    relation_ordinal: u32::try_from(ordinal).ok()?,
                    summary: None,
                    exits: Vec::new(),
                    aggregate: PostconditionAggregate {
                        discharged: true,
                        derivation: None,
                    },
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(FunctionEntailment {
            body_disposition: if receipt.uninhabited {
                CheckedBodyDisposition::Uninhabited {
                    contradiction: DerivationId(0),
                }
            } else {
                CheckedBodyDisposition::Inhabited
            },
            obligations,
            postconditions,
            ..FunctionEntailment::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{IdKind, Spelling, StableText};

    fn spelled(rendering: &str) -> Option<(String, Vec<(IdKind, u64)>)> {
        let mut text = StableText::default();
        text.push(
            rendering,
            &mut |kind, value| match kind {
                IdKind::Function => Some(Spelling::Stable(format!("fn{value}"))),
                IdKind::Declaration if value >= 100 => Some(Spelling::Local),
                IdKind::Declaration => Some(Spelling::Stable(format!("decl{value}"))),
                _ => None,
            },
            &mut |item| (item < 10).then(|| format!("item{item}")),
        )?;
        let named = text.named_since(0).to_vec();
        Some((text.into_text(), named))
    }

    /// Program-wide identities take their stable spelling, local ones their
    /// first-appearance ordinal, and node paths their item's identity.
    #[test]
    fn identities_are_respelled_and_locals_numbered_by_appearance() {
        let (text, named) = spelled(
            "Call { callee: FunctionId(3), binder: DeclarationId(140), again: DeclarationId(140), \
             other: DeclarationId(120), global: DeclarationId(7), at: NodePath { components: [4, 0, 2] }, \
             binding: BindingId(5), name: \"FunctionId(9)\" }",
        )
        .expect("every identity is spelled");
        assert_eq!(
            text,
            "Call { callee: FunctionId(fn3), binder: DeclarationId(local 0), again: \
             DeclarationId(local 0), other: DeclarationId(local 1), global: DeclarationId(decl7), \
             at: NodePath(item4/0/2), binding: BindingId(5), name: \"FunctionId(9)\" }"
        );
        assert_eq!(named, vec![(IdKind::Function, 3), (IdKind::Declaration, 7)]);
    }

    /// A source coordinate is dropped whole, and a capture's source
    /// occurrence is spelled as the node it names.
    #[test]
    fn coordinates_are_dropped_and_captures_spelled_by_node() {
        let mut text = StableText::default();
        text.push(
            "Origin { coordinate: SyntaxCoordinate { source: SourceId(3), start: ByteOffset(120), \
             end: ByteOffset(125) }, capture: Source(88), constructor: Source(DeclarationId(4)) }",
            &mut |kind, value| match kind {
                IdKind::Node => Some(Spelling::Stable(format!("node{value}"))),
                IdKind::Declaration => Some(Spelling::Stable(format!("decl{value}"))),
                _ => None,
            },
            &mut |_| None,
        )
        .expect("every identity is spelled");
        assert_eq!(
            text.into_text(),
            "Origin { coordinate: SyntaxCoordinate, capture: Source(node88), constructor: \
             Source(DeclarationId(decl4)) }"
        );
    }

    /// Two identities one stable spelling covers take distinct spellings in
    /// order of first appearance.
    #[test]
    fn identities_sharing_a_spelling_stay_apart() {
        let mut text = StableText::default();
        text.push(
            "[FunctionId(4), FunctionId(9), FunctionId(4)]",
            &mut |_, _| Some(Spelling::Stable("shared".to_owned())),
            &mut |_| None,
        )
        .expect("every identity is spelled");
        assert_eq!(
            text.into_text(),
            "[FunctionId(shared), FunctionId(shared#1), FunctionId(shared)]"
        );
    }

    /// A dense identity with no spelling, or an unknown identity newtype,
    /// leaves no key.
    #[test]
    fn an_unspelled_identity_leaves_no_key() {
        assert!(spelled("NominalId(2)").is_none());
        assert!(spelled("SomethingId(2)").is_none());
        assert!(spelled("NodePath { components: [12] }").is_none());
        assert!(spelled("Literal(12)").is_some());
    }
}
