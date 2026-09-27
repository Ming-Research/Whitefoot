//! Exact active-specification declaration collection and lexical name resolution.
//!
//! Resolution consumes canonical syntax, builds the specification-defined
//! scope and declaration inventories, and fixes every lexical target. Typed
//! owner/member relationships remain explicit dependent records for the next
//! compiler stage.

mod catalog;
mod engine;
mod kernel;
mod scopes;

#[cfg(test)]
mod tests;

use crate::syntax::NodeId;
use crate::{CanonicalSyntaxUnit, NodePath, SyntaxCoordinate};

pub use engine::resolve;

pub use kernel::{
    CELL_NOMINAL, CELL_NOMINAL_ID, CONTAINER_NOMINAL_CLASS, CONTAINER_NOMINAL_CLASSES,
    CONTAINER_NOMINALS, ContainerNominal, ContainerNominalId, ContainerShape, container_nominal,
    container_nominal_id,
};

/// Returns the exact OP-1 spelling of a resolved operation family.
#[must_use]
pub fn operation_family_spelling(id: OperationFamilyId) -> Option<&'static str> {
    catalog::operation_spelling(id)
}

/// Dense identity of one resolver scope.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScopeId(u32);

impl ScopeId {
    pub(crate) fn from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One scope kind from the active specification's scope-construction matrix.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ScopeKind {
    /// The complete closed compilation unit: the PRE-1 environment every
    /// module extends.
    CompilationUnit,
    /// One module's shared declaration inventory [MOD-3].
    Module,
    /// One source's file-local alias header over its module [MOD-4].
    File,
    /// Type and const generics owned by one declaration.
    DeclarationGenerics,
    /// Parameters, signature suffix, clauses, and body.
    FunctionSignature,
    /// One contract-member signature.
    ContractSignature,
    /// The erased definition and proof-clause scope of one function contract.
    ContractBlock,
    /// A concrete function body.
    FunctionBody,
    /// The statement body nested under an arm, loop, or counted range.
    NestedBody,
    /// Match binders visible to one arm body.
    Arm,
    /// One loop label visible only to that loop body.
    LoopLabel,
    /// One counted label and binder visible only to that counted body.
    CountedRange,
}

/// One resolver scope and its lexical parent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopeRecord {
    id: ScopeId,
    parent: Option<ScopeId>,
    kind: ScopeKind,
    owner: NodePath,
}

impl ScopeRecord {
    /// Returns this scope's dense identity in the resolved unit.
    #[must_use]
    pub const fn id(&self) -> ScopeId {
        self.id
    }

    /// Returns the immediately enclosing scope, if any.
    #[must_use]
    pub const fn parent(&self) -> Option<ScopeId> {
        self.parent
    }

    /// Returns the specification-defined scope kind.
    #[must_use]
    pub const fn kind(&self) -> ScopeKind {
        self.kind
    }

    /// Returns the production node that creates this scope.
    #[must_use]
    pub const fn owner(&self) -> &NodePath {
        &self.owner
    }
}

/// Dense identity of one source declaration event.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclarationId(u32);

impl DeclarationId {
    pub(crate) fn from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Identity of an existing language-known built-in prelude record.
/// This internal identity is independent of its PRE-1 diagnostic ordinal.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuiltinPreludeId(u8);

impl BuiltinPreludeId {
    pub(crate) const BOOL: Self = Self(0);
    pub(crate) const TRUE: Self = Self(1);
    pub(crate) const FALSE: Self = Self(2);
    pub(crate) const OPTION: Self = Self(3);
    pub(crate) const OPTION_TYPE: Self = Self(4);
    pub(crate) const NONE: Self = Self(5);
    pub(crate) const SOME: Self = Self(6);
    pub(crate) const SOME_VALUE: Self = Self(7);
    pub(crate) const RESULT: Self = Self(8);
    pub(crate) const RESULT_VALUE_TYPE: Self = Self(9);
    pub(crate) const RESULT_ERROR_TYPE: Self = Self(10);
    pub(crate) const OK: Self = Self(11);
    pub(crate) const OK_VALUE: Self = Self(12);
    pub(crate) const ERR: Self = Self(13);
    pub(crate) const ERR_ERROR: Self = Self(14);
    pub(crate) const OVERFLOW_TYPE: Self = Self(15);
    pub(crate) const OVERFLOW: Self = Self(16);
    pub(crate) const DIV_ERROR_TYPE: Self = Self(17);
    pub(crate) const DIVIDE_BY_ZERO: Self = Self(18);
    pub(crate) const DIV_OVERFLOW: Self = Self(19);
    pub(crate) const NARROW_ERROR_TYPE: Self = Self(20);
    pub(crate) const NARROW_ERROR: Self = Self(21);
    pub(crate) const INT: Self = Self(22);
    pub(crate) const FLOAT: Self = Self(23);

    /// Returns the internal built-in record index.
    #[must_use]
    pub const fn ordinal(self) -> u8 {
        self.0
    }
}

/// Diagnostic identity in the complete PRE-1 declaration preorder.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PreludeDeclarationId(u32);

impl PreludeDeclarationId {
    pub(crate) fn from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }

    /// Returns the zero-based ordinal in the complete PRE-1 inventory.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.0
    }
}

/// Dense identity of one distinct OP-1 operation-family spelling.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationFamilyId(u16);

impl OperationFamilyId {
    pub(crate) fn from_index(index: usize) -> Option<Self> {
        u16::try_from(index).ok().map(Self)
    }

    /// Returns the operation family's OP-1 first-occurrence ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u16 {
        self.0
    }
}

/// Closed declaration-class order used by active-specification resolution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclarationClass {
    /// Top-level source function.
    Function,
    /// A compile-time function-kind parameter, never a runtime value.
    FunctionParameter,
    /// Top-level immutable named constant.
    NamedConst,
    /// Lexical const generic.
    ConstGeneric,
    /// Parameter, let binding, match binder, or counted-range binder.
    Value,
    /// Lexical type generic.
    GenericType,
    /// Source or prelude nominal type.
    NominalType,
    /// Constructor contributed by a source struct.
    StructConstructor,
    /// Source or prelude enum variant.
    EnumVariant,
    /// One built-in numeric bound: Int or Float.
    NumericBound,
    /// A named parameter-group abbreviation.
    Interface,
    /// A named argument-group abbreviation.
    Binding,
    /// Loop label.
    Label,
    /// One machine-checked invariant fact named by source.
    Invariant,
    /// One distinct OP-1 spelling.
    OperationFamily,
    /// A file-local alias naming a registered module [MOD-4].
    Module,
}

/// Closed resolver collision-domain order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclarationDomain {
    /// Functions, constants, const generics, parameters, lets, and binders.
    LexicalIdentifier,
    /// Generic and nominal types.
    NominalType,
    /// Struct constructors and enum variants.
    Constructor,
    /// Built-in numeric bounds.
    NumericBound,
    /// Loop labels.
    Label,
    /// Machine-checked invariant facts.
    Invariant,
}

impl DeclarationDomain {
    /// [DIAG-1] fixes the conflict-domain order as lexical-IDENT,
    /// nominal-type, constructor, numeric-bound, LABEL, invariant.
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::LexicalIdentifier => 0,
            Self::NominalType => 1,
            Self::Constructor => 2,
            Self::NumericBound => 3,
            Self::Label => 4,
            Self::Invariant => 5,
        }
    }
}

/// Source declaration roles, in the closed order this resolver classifies.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclarationRole {
    /// D01: top-level function.
    Function,
    /// D02: source struct nominal plus constructor.
    Struct,
    /// D03: source enum nominal.
    Enum,
    /// D04: source enum variant.
    Variant,
    /// A named parameter-group declaration.
    Interface,
    /// A named argument-group declaration.
    Binding,
    /// A raw function-kind generic parameter.
    FunctionParameter,
    /// D06: named constant.
    NamedConst,
    /// D07: type generic.
    GenericType,
    /// D08: const generic.
    ConstGeneric,
    /// D10: function or contract-member parameter.
    Parameter,
    /// D11: ordinary lexical let binding.
    Let,
    /// D12: loop label.
    LoopLabel,
    /// D14: match binder.
    MatchBinder,
    /// D15: counted-range binder.
    CountedBinder,
    /// A named invariant fact visible after its checked declaration point.
    Invariant,
    /// A file-local alias from one source's alias header [MOD-4]. Every use
    /// of it resolves to its target's own identity.
    Alias,
}

/// Dependent declaration roles resolved by the semantic owner type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DependentDeclarationRole {
    /// X01: source struct field.
    Field,
    /// X02: source enum-variant field.
    VariantField,
    /// One function-kind member of a named parameter group.
    FunctionMember,
}

/// Lexical-use roles retained by name resolution.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LexicalUseRole {
    /// U01: nominal or generic type.
    Type,
    /// U02: built-in numeric bound.
    GenericBound,
    /// An interface group in a header or forwarding/member application.
    FormalGroup,
    /// An explicit argument name, admitting a type or group abbreviation.
    TypeArgument,
    /// U04: struct or enum construction.
    Construct,
    /// The owner enum of a type-owned variant construction, `Sign::Neg()`
    /// [TYPE-6].
    VariantOwner,
    /// The leading enum variant of an FN-9 selector.
    EnsuresVariant,
    /// U09: reference parameter at the root of a state-effect path [EFF-1].
    EffectRoot,
    /// An index or range endpoint supplied inside a state-effect path. It is
    /// an IDENT that must resolve to a value parameter of the same callable
    /// and is evaluated once at the call [EFF-1].
    EffectIndex,
    /// U11: break target.
    BreakLabel,
    /// U12: constant-expression identifier.
    Const,
    /// U13: constant-value identifier.
    ConstValue,
    /// U14: place base.
    PlaceBase,
    /// U15: identifier callee.
    IdentifierCallee,
    /// U16: dotted operation callee.
    OperationCallee,
    /// A source function or function parameter used as an explicit argument.
    FunctionBinding,
    /// U18: generic suffix in `0_T` or `1_T`.
    GenericNumericSuffix,
    /// One local integer value named by an INV-1 proof-only affine factor.
    InvariantValue,
    /// One local integer value named by a PRF-1 finite source-proof factor.
    ProofValue,
    /// One earlier named invariant selected by a local `use` step.
    InvariantFact,
}

/// Deferred member and field uses resolved by the semantic owner type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DeferredUseRole {
    /// X04: construction field or named-call argument.
    FieldInitializer,
    /// X05: match field label.
    MatchField,
    /// The variant label of a match arm, resolved against the scrutinee's
    /// already known enum type [TYPE-6].
    ArmVariant,
    /// X06: projected field.
    ProjectedField,
    /// The variant TYPEID of a payload `psuffix` or `epsuffix`, `p.Some.value`
    /// [GRAM-5, EFF-1]. Which variant a place's enum type has is the owner
    /// type's judgment, exactly as the paired field name is.
    PayloadVariant,
    /// The member name on the left side of a named binding.
    FunctionBinding,
    /// The member selected by a qualified group call.
    FunctionMember,
    /// A statically selected field after an effect-path root.
    EffectField,
}

/// Exact source origin of one resolver role.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceOrigin {
    node: NodePath,
    coordinate: SyntaxCoordinate,
    /// The owning production's complete extent, so a diagnostic naming this
    /// origin can quote the declaration or use it belongs to.
    extent: SyntaxCoordinate,
    role_ordinal: u32,
    subtoken_ordinal: u32,
}

impl SourceOrigin {
    /// Returns the production node owning this role.
    #[must_use]
    pub const fn node(&self) -> &NodePath {
        &self.node
    }

    /// Returns the exact source coordinate of the role spelling.
    #[must_use]
    pub const fn coordinate(&self) -> SyntaxCoordinate {
        self.coordinate
    }

    /// Returns the complete source extent of the production owning this role.
    #[must_use]
    pub const fn extent(&self) -> SyntaxCoordinate {
        self.extent
    }

    /// Returns the direct-carrier ordinal within the owner production.
    #[must_use]
    pub const fn role_ordinal(&self) -> u32 {
        self.role_ordinal
    }

    /// Returns zero for a complete carrier or the embedded subtoken ordinal.
    #[must_use]
    pub const fn subtoken_ordinal(&self) -> u32 {
        self.subtoken_ordinal
    }
}

/// Origin of a declaration participating in a diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeclarationOrigin {
    /// One source declaration.
    Source(SourceOrigin),
    /// One normative PRE-1 record.
    ///
    /// x1 retires the third `Container` form this enum used to carry. The
    /// four storage nominals are PRE-1 records now [TYPE-2], so a collision
    /// with one names the prelude declaration it is, and [DIAG-1]'s two
    /// enumerated origins are again the complete set.
    Prelude(PreludeDeclarationId),
}

/// [MOD-3] Where an item is declared, by identities no graph row position,
/// file name or dense id enters: one module's interface or implementation
/// records, or the compiler's own PRE-1 records.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemHome {
    /// A registered module, by package and path, and which of its records
    /// declare the item: its interface declaration and its definition are
    /// two declarations of one function.
    Module {
        /// The package the module belongs to [MOD-10].
        package: crate::Package,
        /// The module's path within its package.
        path: Vec<String>,
        /// The records that declare the item.
        record: crate::SourceRole,
    },
    /// The compiler's own PRE-1 records.
    Prelude,
}

impl ItemHome {
    /// The home of the items one source declares.
    fn of(file: &crate::SourceFile, bundle: &crate::SourceBundle) -> Option<Self> {
        if file.prelude().is_some() {
            return Some(Self::Prelude);
        }
        let module = bundle.module(file.module())?;
        Some(Self::Module {
            package: module.package(),
            path: module.path().to_vec(),
            record: file.role(),
        })
    }
}

impl core::fmt::Display for ItemHome {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Module {
                package,
                path,
                record,
            } => {
                formatter.write_str(package.qualifier())?;
                for component in path {
                    write!(formatter, "::{component}")?;
                }
                if *record == crate::SourceRole::Interface {
                    formatter.write_str(" interface")?;
                }
                Ok(())
            }
            Self::Prelude => formatter.write_str("prelude"),
        }
    }
}

/// The stable identity resolution mints for one item of the unit
/// (`design/compiler/incremental-compilation.md`): the declaration that heads
/// it, by home, role and spelling, or the alias it binds. Adding, removing or
/// reordering other items changes no item's key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemKey {
    /// An item headed by a function, struct, enum, interface, binding or
    /// constant declaration [GRAM-2].
    Declared {
        /// Where it is declared.
        home: ItemHome,
        /// The heading declaration's role.
        role: DeclarationRole,
        /// The heading declaration's spelling.
        spelling: String,
    },
    /// A file-local alias [MOD-4], by the logical path of the source whose
    /// alias header binds it and its spelling.
    Alias {
        /// The logical path of the source that binds it.
        source: String,
        /// The alias's spelling.
        spelling: String,
    },
}

impl core::fmt::Display for ItemKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Declared {
                home,
                role,
                spelling,
            } => write!(formatter, "{home}::{spelling}#{role:?}"),
            Self::Alias { source, spelling } => write!(formatter, "alias {source}::{spelling}"),
        }
    }
}

/// The stable identity resolution mints for one declaration: an item's own
/// heading declaration is that item's key, and every other declaration is
/// placed within its item by the path from the item's node to the declaring
/// node and its ordinal among that node's roles. An edit outside an item
/// changes no key within it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeclarationKey {
    /// The declaration that heads an item.
    Item(ItemKey),
    /// A declaration within an item.
    Local {
        /// The item that holds it.
        item: ItemKey,
        /// The child ordinals from the item's node to the declaring node.
        path: Vec<u32>,
        /// Its role ordinal and subtoken ordinal within the declaring node.
        ordinal: (u32, u32),
    },
}

impl DeclarationKey {
    /// The item that holds the declaration.
    #[must_use]
    pub const fn item(&self) -> &ItemKey {
        match self {
            Self::Item(item) | Self::Local { item, .. } => item,
        }
    }
}

impl core::fmt::Display for DeclarationKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Item(item) => write!(formatter, "{item}"),
            Self::Local {
                item,
                path,
                ordinal: (role, subtoken),
            } => {
                write!(formatter, "{item}")?;
                for component in path {
                    write!(formatter, "/{component}")?;
                }
                write!(formatter, "#{role}.{subtoken}")
            }
        }
    }
}

/// The stable identity of one syntax node, relative to the item that holds
/// it: the item's key and the child ordinals from the item's node down.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OccurrenceKey {
    /// The item that holds the node.
    pub item: ItemKey,
    /// The child ordinals from the item's node to this node.
    pub path: Vec<u32>,
}

impl core::fmt::Display for OccurrenceKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}", self.item)?;
        for component in &self.path {
            write!(formatter, "/{component}")?;
        }
        Ok(())
    }
}

/// One source declaration event and its lookup entries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationRecord {
    id: DeclarationId,
    role: DeclarationRole,
    spelling: String,
    origin: SourceOrigin,
    scope: ScopeId,
    classes: Vec<DeclarationClass>,
    diagnostic_origins: Vec<(DeclarationClass, PreludeDeclarationId)>,
    /// The module whose inventory holds this declaration, or `None` for a
    /// PRE-1 record [MOD-3].
    module: Option<crate::ModuleId>,
    /// Whether an interface marks it `public` [MOD-6].
    public: bool,
    /// Its stable identity.
    key: DeclarationKey,
}

impl DeclarationRecord {
    fn diagnostic_origin(&self, class: DeclarationClass) -> DeclarationOrigin {
        self.diagnostic_origins
            .iter()
            .find_map(|(candidate, origin)| (*candidate == class).then_some(*origin))
            .map_or_else(
                || DeclarationOrigin::Source(self.origin.clone()),
                DeclarationOrigin::Prelude,
            )
    }

    /// Returns this declaration's dense identity.
    #[must_use]
    pub const fn id(&self) -> DeclarationId {
        self.id
    }

    /// Returns its exact grammar role.
    #[must_use]
    pub const fn role(&self) -> DeclarationRole {
        self.role
    }

    /// Returns the exact source spelling, including any sigil.
    #[must_use]
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    /// Returns its source origin.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    /// Returns the scope in which it is declared.
    #[must_use]
    pub const fn scope(&self) -> ScopeId {
        self.scope
    }

    /// Returns its one or two grammar-selected lookup classes.
    #[must_use]
    pub fn classes(&self) -> &[DeclarationClass] {
        &self.classes
    }

    /// Returns the module whose inventory holds it; `None` for a PRE-1
    /// record [MOD-3].
    #[must_use]
    pub const fn module(&self) -> Option<crate::ModuleId> {
        self.module
    }

    /// Reports whether an interface publishes it [MOD-6].
    #[must_use]
    pub const fn is_public(&self) -> bool {
        self.public
    }

    /// Returns the stable identity resolution minted for it.
    #[must_use]
    pub const fn key(&self) -> &DeclarationKey {
        &self.key
    }
}

/// One typed-owner-dependent declaration retained for the next stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependentDeclarationRecord {
    role: DependentDeclarationRole,
    spelling: String,
    origin: SourceOrigin,
}

impl DependentDeclarationRecord {
    /// Returns the dependent role.
    #[must_use]
    pub const fn role(&self) -> DependentDeclarationRole {
        self.role
    }

    /// Returns its exact spelling.
    #[must_use]
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    /// Returns its source origin.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }
}

/// One successful lexical target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResolvedTarget {
    /// One class contributed by a source declaration event.
    Source {
        /// Source declaration.
        declaration: DeclarationId,
        /// Selected grammar class.
        class: DeclarationClass,
    },
    /// One normative PRE-1 lookup entry.
    Prelude(BuiltinPreludeId),
    /// One exact OP-1 operation family.
    Operation(OperationFamilyId),
    /// One [TYPE-9] compiler-owned storage nominal, admitted at a `type`
    /// TYPEID in every unit.
    Container(ContainerNominalId),
}

/// One lexical use and its exact target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexicalUseRecord {
    role: LexicalUseRole,
    spelling: String,
    origin: SourceOrigin,
    target: ResolvedTarget,
}

impl LexicalUseRecord {
    /// Returns the grammar-selected use role.
    #[must_use]
    pub const fn role(&self) -> LexicalUseRole {
        self.role
    }

    /// Returns the complete name spelling, or the bare generic suffix.
    #[must_use]
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    /// Returns the source origin.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    /// Returns the unique resolved target.
    #[must_use]
    pub const fn target(&self) -> ResolvedTarget {
        self.target
    }
}

/// One owner/member use deliberately deferred to typed checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeferredUseRecord {
    role: DeferredUseRole,
    spelling: String,
    origin: SourceOrigin,
}

impl DeferredUseRecord {
    /// Returns the deferred role.
    #[must_use]
    pub const fn role(&self) -> DeferredUseRole {
        self.role
    }

    /// Returns the complete carrier spelling.
    #[must_use]
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    /// Returns the source origin.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }
}

/// Grammar class of one private FN-9 selector record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PostconditionSelectorClass {
    Plain,
    Variant,
}

/// One FN-9 result-datum candidate. It is deliberately not a declaration or binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionCandidateRecord {
    pub(crate) spelling: String,
    pub(crate) origin: SourceOrigin,
    pub(crate) paired_field: Option<String>,
    pub(crate) live_conflicts: Vec<SourceOrigin>,
    pub(crate) later_local_collision: Option<SourceOrigin>,
}

/// One written variant-selector field and its candidate binder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionFieldRecord {
    pub(crate) spelling: String,
    pub(crate) origin: SourceOrigin,
    pub(crate) candidate: PostconditionCandidateRecord,
}

/// One in-clause pbase provisionally owned by its selector rather than TYPE-6.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionSelectorUseRecord {
    pub(crate) spelling: String,
    pub(crate) origin: SourceOrigin,
}

/// Private resolver handoff for one structurally admitted FN-9 clause.
///
/// Ordinary entry uses are linked only provisionally. The semantic FN-9
/// admission pass activates them after concrete signature substitution; an
/// invalid selector therefore wins before any deferred entry lookup issue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PostconditionResolutionRecord {
    pub(crate) function: NodePath,
    pub(crate) block: NodePath,
    pub(crate) selector: NodePath,
    pub(crate) class: PostconditionSelectorClass,
    /// Every declared result ordinal's binder, in written order [GRAM-2].
    /// A declaration writing one result has exactly one entry, and every
    /// clause of either class may name any of them [CALL-4].
    pub(crate) result_binders: Vec<PostconditionCandidateRecord>,
    /// The route's written ordinal binder, `when b is V(f: r):`, when the
    /// clause writes one [CALL-4].
    pub(crate) route_ordinal: Option<String>,
    pub(crate) fields: Vec<PostconditionFieldRecord>,
    pub(crate) variant_target: Option<ResolvedTarget>,
    pub(crate) provisional_uses: Vec<LexicalUseRecord>,
    pub(crate) selector_uses: Vec<PostconditionSelectorUseRecord>,
    pub(crate) entry_inventory_issue: Option<ResolutionIssue>,
    pub(crate) entry_resolution_issue: Option<ResolutionIssue>,
}

/// One record in the complete PRE-1 diagnostic inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreludeDeclarationRecord {
    id: PreludeDeclarationId,
    spelling: String,
    class: Option<DeclarationClass>,
}

impl PreludeDeclarationRecord {
    #[must_use]
    /// Returns this record's complete PRE-1 diagnostic ordinal.
    pub const fn id(&self) -> PreludeDeclarationId {
        self.id
    }

    #[must_use]
    /// Returns the declaration or owner-local record spelling.
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    #[must_use]
    /// Returns its whole-unit lookup class, or None for an owner-local record.
    pub const fn lookup_class(&self) -> Option<DeclarationClass> {
        self.class
    }
}

/// An existing built-in declaration used by ordinary lookup.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BuiltinPreludeDeclarationRecord {
    id: BuiltinPreludeId,
    spelling: &'static str,
    class: Option<DeclarationClass>,
}

/// Numbered rule owning one resolver rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolutionRule {
    /// Reserved declaration name.
    Form3,
    /// Generic numeric suffix.
    Form5,
    /// A misplaced or repeated heap declaration.
    Gram2,
    /// Match-binder freshness.
    Gram10,
    /// Type or place lookup.
    Type5,
    /// Namespace collision, constructor, or label lookup.
    Type6,
    /// A `set` target name that resolves to nothing.
    Set1,
    /// Constant-expression lookup.
    Const1,
    /// Constant-value lookup.
    Const2,
    /// Reference-parameter-rooted state-effect lookup.
    Eff1,
    /// Operation-family or callee lookup.
    Op1,
    /// Contract lookup.
    Fn3,
    /// Function binding lookup.
    Fn4,
    /// Requires-block structural admission.
    Fn8,
    /// Ensures-block structural and selector admission.
    Fn9,
    /// Invariant declaration names and proof-only target-value lookup.
    Inv1,
    /// Finite source-certificate relation-value lookup.
    Prf1,
    /// A module's shared declaration inventory.
    Mod3,
    /// A file-local alias header.
    Mod4,
    /// A qualified reference: module path, dependency edge and access.
    Mod5,
    /// Publication: where `public` and `readonly` may be written.
    Mod6,
    /// Interface and implementation correspondence.
    Mod7,
    /// A module program's heap declaration spelling.
    Mod9,
    /// The standard library's own qualifier.
    Mod10,
}

impl ResolutionRule {
    /// Returns the exact numbered rule spelling from the active kernel specification.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Form3 => "FORM-3",
            Self::Form5 => "FORM-5",
            Self::Gram2 => "GRAM-2",
            Self::Gram10 => "GRAM-10",
            Self::Type5 => "TYPE-5",
            Self::Type6 => "TYPE-6",
            Self::Set1 => "SET-1",
            Self::Const1 => "CONST-1",
            Self::Const2 => "CONST-2",
            Self::Eff1 => "EFF-1",
            Self::Op1 => "OP-1",
            Self::Fn3 => "FN-3",
            Self::Fn4 => "FN-4",
            Self::Fn8 => "FN-8",
            Self::Fn9 => "FN-9",
            Self::Inv1 => "INV-1",
            Self::Prf1 => "PRF-1",
            Self::Mod3 => "MOD-3",
            Self::Mod4 => "MOD-4",
            Self::Mod5 => "MOD-5",
            Self::Mod6 => "MOD-6",
            Self::Mod7 => "MOD-7",
            Self::Mod9 => "MOD-9",
            Self::Mod10 => "MOD-10",
        }
    }
}

/// Which closed reserved-name set owns one spelling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservedNameClass {
    /// A distinct dotless OP-1 family.
    DotlessOperation,
    /// One FORM-3 operation-mode suffix word.
    ModeWord,
}

/// Declaration roles covered by OP-1's reserved-lower-name inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservedDeclarationRole {
    /// Top-level function.
    Function,
    /// Top-level named constant.
    NamedConst,
    /// Function or contract-member parameter.
    Parameter,
    /// Lexical let binding.
    Let,
    /// Contract-block definition binder, the `define` of a requires and
    /// ensures block, which FORM-3 lists as its own carrier role.
    ContractDefinition,
    /// Counted-range binder.
    ForBinder,
    /// Match binder.
    MatchBinder,
    /// Plain FN-9 result-selector candidate.
    PlainResultSelector,
    /// Variant-form FN-9 result-selector candidate.
    VariantResultSelector,
    /// Struct field.
    Field,
    /// Enum-variant field.
    VariantField,
}

/// Why an FN-8/FN-9 contract block was rejected by early admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractShapeIssue {
    /// The block contains definitions but no proof clause.
    MissingClause,
}

/// One declaration conflict carried by a TYPE-6 issue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationConflict {
    pub(crate) domain: DeclarationDomain,
    pub(crate) class: DeclarationClass,
    pub(crate) origin: DeclarationOrigin,
}

impl DeclarationConflict {
    /// Returns the fixed TYPE-6 collision domain.
    #[must_use]
    pub const fn domain(&self) -> DeclarationDomain {
        self.domain
    }

    /// Returns the conflicting declaration class.
    #[must_use]
    pub const fn class(&self) -> DeclarationClass {
        self.class
    }

    /// Returns the conflicting declaration origin.
    #[must_use]
    pub const fn origin(&self) -> &DeclarationOrigin {
        &self.origin
    }
}

/// Structured payload of one deterministic resolver rejection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolutionIssueKind {
    /// Early FN-8/FN-9 contract structural-admission failure.
    ContractShape(ContractShapeIssue),
    /// A `heap_decl` that is not the first item of the first source record, or
    /// a second one anywhere in the unit [GRAM-2].
    MisplacedHeapDeclaration {
        /// The admitted leading declaration, when the unit has one.
        admitted: Option<SourceOrigin>,
    },
    /// A declaration uses a derived reserved lower name.
    ReservedName {
        /// The complete declaration or result-candidate spelling.
        spelling: String,
        /// Exact declaration role covered by the reserved-name rule.
        declaration_role: ReservedDeclarationRole,
        /// Owning reserved set.
        class: ReservedNameClass,
        /// Ordinal inside that reserved set.
        inventory_ordinal: u16,
    },
    /// GRAM-10 freshness failed before the binder became a declaration.
    MatchBinderFreshness {
        /// Binder spelling.
        spelling: String,
        /// Paired source field label.
        paired_field: String,
        /// Earlier equal binder in this arm, if any.
        earlier_binder: Option<SourceOrigin>,
        /// Live lexical-IDENT declarations at arm entry.
        arm_entry_conflicts: Vec<SourceOrigin>,
    },
    /// A PRE-1 collision, duplicate, redeclaration, or live shadow.
    DeclarationCollision {
        /// Offending spelling.
        spelling: String,
        /// Ordered nonempty conflicts.
        conflicts: Vec<DeclarationConflict>,
        /// The repair the colliding situation admits.
        ///
        /// The four situations differ in what a writer can do about them, and
        /// the conflict list locates the other declaration without ever saying
        /// which situation this is.
        mechanical_fix: &'static str,
    },
    /// Admissible declarations exist in the candidate universe but are hidden.
    InvisibleUse {
        /// Use spelling.
        spelling: String,
        /// Use role.
        role: LexicalUseRole,
        /// Ordered admissible classes.
        admissible: Vec<DeclarationClass>,
        /// Ordered invisible declaration origins.
        origins: Vec<DeclarationOrigin>,
    },
    /// Labels with this spelling exist in the function but do not enclose use.
    NonEnclosingLabel {
        /// Complete label spelling.
        spelling: String,
        /// The label-use role, retained explicitly in the diagnostic payload.
        role: LexicalUseRole,
        /// Ordered current-function label origins.
        origins: Vec<DeclarationOrigin>,
    },
    /// A module program writes `program no_heap`, which only a source
    /// bundle admits; an entry states its heap requirement instead [MOD-9].
    ModuleProgramHeapDeclaration,
    /// An alias item follows a non-alias item of its source [MOD-4].
    MisplacedAlias,
    /// An alias target names nothing it may bind [MOD-4].
    InvalidAliasTarget {
        /// The alias spelling.
        spelling: String,
        /// The complete written target path.
        target: String,
        /// Why the target is refused.
        reason: &'static str,
    },
    /// A standard library record writes `std`, where the library names
    /// itself `pkg` [MOD-10].
    LibraryNamesItself {
        /// The written path.
        path: String,
    },
    /// A qualified path's module prefix names no registered module [MOD-5].
    UnknownModule {
        /// The written module path, `pkg::a::b`.
        path: String,
    },
    /// A reference names another module that its source module's graph row
    /// does not list as a direct dependency [MOD-5].
    MissingModuleEdge {
        /// The referring module.
        from: String,
        /// The referenced module.
        to: String,
    },
    /// A qualified name names nothing in its module's inventory [MOD-5].
    QualifiedNameNotFound {
        /// The final spelling.
        spelling: String,
        /// The module searched.
        module: String,
        /// Use role.
        role: LexicalUseRole,
    },
    /// A reference from another module names a private declaration [MOD-5].
    PrivateDeclaration {
        /// The referenced spelling.
        spelling: String,
        /// Its declaring module.
        module: String,
    },
    /// A type-owned variant construction names an owner that is not a source
    /// enum, or a variant the owner does not declare [TYPE-6].
    UnknownOwnedVariant {
        /// The variant spelling.
        spelling: String,
        /// Why the owner selects no variant.
        reason: &'static str,
    },
    /// `public` or `readonly` where it creates no access route [MOD-6].
    MisplacedPublication {
        /// Why the marker is refused.
        reason: &'static str,
    },
    /// A public declaration's signature, type, contract or formal names a
    /// private declaration of its own module, which clients cannot access
    /// [MOD-6].
    PrivateInPublicSignature {
        /// The private declaration's spelling.
        spelling: String,
    },
    /// An interface function item with a body, an implementation function
    /// without one, or a declaration without its one definition [MOD-7].
    Correspondence {
        /// The function spelling.
        spelling: String,
        /// What the pairing requires.
        reason: &'static str,
    },
    /// No visible declaration in the admissible classes exists.
    UnresolvedUse {
        /// Use spelling.
        spelling: String,
        /// Use role.
        role: LexicalUseRole,
        /// Ordered admissible classes.
        admissible: Vec<DeclarationClass>,
        /// Visible exact-spelling classes in the candidate universe.
        available: Vec<DeclarationClass>,
    },
    /// [SET-1] a bare `set` target whose name resolves to nothing, which
    /// declares nothing.
    UndeclaredSetTarget {
        /// The target name.
        spelling: String,
        /// The repair [DIAG-1].
        mechanical_fix: String,
    },
}

/// The first active-specification resolver rejection in specified stage and event order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionIssue {
    pub(crate) rule: ResolutionRule,
    pub(crate) origin: SourceOrigin,
    pub(crate) kind: ResolutionIssueKind,
}

impl ResolutionIssue {
    /// Returns the numbered rule owning this rejection.
    #[must_use]
    pub const fn rule(&self) -> ResolutionRule {
        self.rule
    }

    /// Returns the source node and coordinate selected by DIAG-1.
    #[must_use]
    pub const fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    /// Returns the complete structured diagnostic payload.
    #[must_use]
    pub const fn kind(&self) -> &ResolutionIssueKind {
        &self.kind
    }
}

/// Trusted resolver invariant failure, never a source-language rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolutionCompilerFailure {
    /// Canonical topology was inconsistent with its published invariants.
    InvalidCanonicalTree,
    /// A name-shaped grammar carrier was absent or classified more than once.
    UnclassifiedNameRole,
    /// A role-bearing token had an impossible grammar shape.
    InvalidRoleShape,
    /// A source name could not be represented as its required ASCII spelling.
    InvalidNameEncoding,
    /// Scope construction did not cover the complete tree.
    InvalidScopeTree,
    /// Successful lookup produced zero or multiple targets.
    AmbiguousResolution,
    /// A dense identity, ordinal, or coordinate calculation overflowed.
    CounterOverflow,
    /// Two declarations of a resolved unit received one key, which would
    /// merge their identities for every stage that reads keys.
    DuplicateDeclarationKey,
}

/// Canonical syntax plus complete active-specification lexical resolution tables.
#[derive(Debug)]
pub struct ResolvedSyntaxUnit {
    syntax: CanonicalSyntaxUnit,
    scopes: Vec<ScopeRecord>,
    prelude: Vec<PreludeDeclarationRecord>,
    declarations: Vec<DeclarationRecord>,
    dependent_declarations: Vec<DependentDeclarationRecord>,
    lexical_uses: Vec<LexicalUseRecord>,
    deferred_uses: Vec<DeferredUseRecord>,
    postconditions: Vec<PostconditionResolutionRecord>,
    interface_functions: Vec<InterfaceFunction>,
    by_node: NodeRecords,
    /// The key of each item, by its ordinal among the root's children;
    /// `None` for `program no_heap`, which heads no declaration.
    items: Vec<Option<ItemKey>>,
}

/// The positions of one record list's entries, grouped by the node that owns
/// each entry's origin and kept in record order within a node, so a reader
/// holding a node finds its records without scanning the list.
#[derive(Debug, Default)]
struct NodeIndex {
    /// Node `n`'s entries are `entries[starts[n]..starts[n + 1]]`.
    starts: Vec<usize>,
    entries: Vec<usize>,
}

impl NodeIndex {
    /// Groups the records by owner; `owners` yields each record's owner node
    /// index in record order, or `None` for an origin that names no node.
    fn build(nodes: usize, owners: impl Iterator<Item = Option<usize>> + Clone) -> Self {
        let mut starts = vec![0; nodes + 1];
        for owner in owners.clone().flatten() {
            starts[owner + 1] += 1;
        }
        for node in 0..nodes {
            starts[node + 1] += starts[node];
        }
        let mut next = starts.clone();
        let mut entries = vec![0; starts[nodes]];
        for (record, owner) in owners.enumerate() {
            if let Some(owner) = owner {
                entries[next[owner]] = record;
                next[owner] += 1;
            }
        }
        Self { starts, entries }
    }

    fn at(&self, node: NodeId) -> &[usize] {
        match (
            self.starts.get(node.index()),
            self.starts.get(node.index() + 1),
        ) {
            (Some(start), Some(end)) => &self.entries[*start..*end],
            _ => &[],
        }
    }
}

/// The record lists a checker reads by node, each indexed by its owner node.
#[derive(Debug, Default)]
pub(crate) struct NodeRecords {
    declarations: NodeIndex,
    dependent_declarations: NodeIndex,
    lexical_uses: NodeIndex,
    deferred_uses: NodeIndex,
}

impl NodeRecords {
    /// Indexes each list by the node its records' origins name, finding a
    /// node by its path through `node_of`.
    pub(crate) fn build(
        nodes: usize,
        node_of: impl Fn(&NodePath) -> Option<usize>,
        declarations: &[DeclarationRecord],
        dependent_declarations: &[DependentDeclarationRecord],
        lexical_uses: &[LexicalUseRecord],
        deferred_uses: &[DeferredUseRecord],
    ) -> Self {
        Self {
            declarations: NodeIndex::build(
                nodes,
                declarations
                    .iter()
                    .map(|record| node_of(record.origin().node())),
            ),
            dependent_declarations: NodeIndex::build(
                nodes,
                dependent_declarations
                    .iter()
                    .map(|record| node_of(record.origin().node())),
            ),
            lexical_uses: NodeIndex::build(
                nodes,
                lexical_uses
                    .iter()
                    .map(|record| node_of(record.origin().node())),
            ),
            deferred_uses: NodeIndex::build(
                nodes,
                deferred_uses
                    .iter()
                    .map(|record| node_of(record.origin().node())),
            ),
        }
    }
}

/// One interface function declaration and the definition that implements
/// it, if its module has one yet [MOD-7].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterfaceFunction {
    declaration: DeclarationId,
    definition: Option<DeclarationId>,
}

impl InterfaceFunction {
    /// Returns the body-less interface declaration.
    #[must_use]
    pub const fn declaration(&self) -> DeclarationId {
        self.declaration
    }

    /// Returns the definition, or `None` while the declaration is pending.
    #[must_use]
    pub const fn definition(&self) -> Option<DeclarationId> {
        self.definition
    }
}

impl ResolvedSyntaxUnit {
    /// [MOD-6, MOD-8] the read-only rendering of one module's resolved public
    /// interface: its public declarations and the complete definitions they
    /// reach, every name printed as its qualified identity and no `doc`
    /// entry, so that comparing two revisions' renderings conservatively
    /// detects every semantic and representation change.
    ///
    /// # Errors
    ///
    /// Returns a compiler invariant failure when the canonical tree or the
    /// resolution records are inconsistent.
    pub fn render_interface(
        &self,
        module: crate::ModuleId,
    ) -> Result<String, ResolutionCompilerFailure> {
        engine::render_interface(
            &self.syntax.finalized.topology,
            self.syntax.classified_bundle(),
            &self.declarations,
            &self.lexical_uses,
            module,
        )
    }

    /// Returns every interface function declaration with its definition
    /// [MOD-7].
    #[must_use]
    pub fn interface_functions(&self) -> &[InterfaceFunction] {
        &self.interface_functions
    }

    /// Returns the source-bound canonical syntax consumed by this stage.
    #[must_use]
    pub const fn syntax(&self) -> &CanonicalSyntaxUnit {
        &self.syntax
    }

    /// Returns the complete scope tree.
    #[must_use]
    pub fn scopes(&self) -> &[ScopeRecord] {
        &self.scopes
    }

    /// Returns the complete PRE-1 diagnostic records in normative preorder.
    #[must_use]
    pub fn prelude_declarations(&self) -> &[PreludeDeclarationRecord] {
        &self.prelude
    }

    /// Returns one PRE-1 record by its normative identity.
    #[must_use]
    pub fn prelude_declaration(
        &self,
        id: PreludeDeclarationId,
    ) -> Option<&PreludeDeclarationRecord> {
        self.prelude.get(id.ordinal() as usize)
    }

    /// Returns all source declaration events.
    #[must_use]
    pub fn declarations(&self) -> &[DeclarationRecord] {
        &self.declarations
    }

    /// Returns one source declaration by its resolved identity.
    #[must_use]
    pub fn declaration(&self, id: DeclarationId) -> Option<&DeclarationRecord> {
        self.declarations.get(id.index())
    }

    /// Returns the key of the item the root's child `ordinal` is, or `None`
    /// for an item that heads no declaration.
    #[must_use]
    pub fn item_key(&self, ordinal: u32) -> Option<&ItemKey> {
        self.items.get(ordinal as usize)?.as_ref()
    }

    /// Returns the stable identity of the node at `path`, relative to the
    /// item that holds it; `None` for the root and within an item that heads
    /// no declaration.
    #[must_use]
    pub fn occurrence_key(&self, path: &NodePath) -> Option<OccurrenceKey> {
        let (first, rest) = path.components().split_first()?;
        Some(OccurrenceKey {
            item: self.item_key(*first)?.clone(),
            path: rest.to_vec(),
        })
    }

    /// Returns the dependent declarations.
    #[must_use]
    pub fn dependent_declarations(&self) -> &[DependentDeclarationRecord] {
        &self.dependent_declarations
    }

    /// Returns every successful lexical use.
    #[must_use]
    pub fn lexical_uses(&self) -> &[LexicalUseRecord] {
        &self.lexical_uses
    }

    /// Returns the deferred owner/member uses.
    #[must_use]
    pub fn deferred_uses(&self) -> &[DeferredUseRecord] {
        &self.deferred_uses
    }

    /// The declarations whose origin is `node`, in record order.
    pub(crate) fn declarations_at(&self, node: NodeId) -> impl Iterator<Item = &DeclarationRecord> {
        let records = &self.declarations;
        self.by_node
            .declarations
            .at(node)
            .iter()
            .map(|index| &records[*index])
    }

    /// The dependent declarations whose origin is `node`, in record order.
    pub(crate) fn dependent_declarations_at(
        &self,
        node: NodeId,
    ) -> impl Iterator<Item = &DependentDeclarationRecord> {
        let records = &self.dependent_declarations;
        self.by_node
            .dependent_declarations
            .at(node)
            .iter()
            .map(|index| &records[*index])
    }

    /// The lexical uses whose origin is `node`, in record order.
    pub(crate) fn lexical_uses_at(&self, node: NodeId) -> impl Iterator<Item = &LexicalUseRecord> {
        let records = &self.lexical_uses;
        self.by_node
            .lexical_uses
            .at(node)
            .iter()
            .map(|index| &records[*index])
    }

    /// The deferred uses whose origin is `node`, in record order.
    pub(crate) fn deferred_uses_at(
        &self,
        node: NodeId,
    ) -> impl Iterator<Item = &DeferredUseRecord> {
        let records = &self.deferred_uses;
        self.by_node
            .deferred_uses
            .at(node)
            .iter()
            .map(|index| &records[*index])
    }

    pub(crate) fn postconditions(&self) -> &[PostconditionResolutionRecord] {
        &self.postconditions
    }

    /// Consumes resolution and returns the underlying canonical syntax.
    #[must_use]
    pub fn into_syntax(self) -> CanonicalSyntaxUnit {
        self.syntax
    }
}

/// Failure-atomic outcome of active-specification lexical resolution.
#[derive(Debug)]
pub enum ResolutionOutcome {
    /// The complete scope, declaration, lexical-use, and deferred-role tables.
    Complete(ResolvedSyntaxUnit),
    /// The first spec-defined FN-8, inventory, or lookup rejection.
    SourceIssue {
        /// Canonical syntax retained for diagnostics or caller policy.
        syntax: CanonicalSyntaxUnit,
        /// Deterministic resolver issue.
        issue: ResolutionIssue,
    },
    /// A trusted compiler invariant failed.
    CompilerFailure {
        /// Canonical syntax retained for debugging.
        syntax: CanonicalSyntaxUnit,
        /// Internal failure class.
        failure: ResolutionCompilerFailure,
    },
}
