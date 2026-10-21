//! Target emission structure shared by whole-module and fragment rendering.
//!
//! This is a printing model, not an optimization IR. Instructions remain text;
//! block identity and references which other emission consumers need do not.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};

use crate::IrBlockId;

use super::emitter::BackendFailure;

/// A deferred definition's literal text and block-scoped local identifiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DeferredPart {
    Text(String),
    Local(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum BodyPart {
    Text(String),
    Block {
        label: String,
        entry: bool,
    },
    Incoming {
        value: String,
        predecessor: IrBlockId,
    },
    DeferredLoad {
        canonical: String,
        label: String,
    },
    Terminator {
        label: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct DeferredRequests {
    requested: BTreeSet<(String, String)>,
    pending: Vec<(String, String)>,
    invalid: bool,
}

/// A function's emitted blocks and deferred incoming edges. Recording a label
/// when its block is opened gives an IR predecessor its actual target exit.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FunctionBody {
    parts: Vec<BodyPart>,
    current_label: Option<String>,
    exits: Vec<Option<String>>,
    block_operands: BTreeMap<String, Vec<DeferredPart>>,
    deferred: RefCell<DeferredRequests>,
    pub(crate) references: References,
}

impl FunctionBody {
    pub(crate) fn register_block_operand(
        &mut self,
        canonical: String,
        definition: Vec<DeferredPart>,
    ) {
        if self.current_label.is_some() || self.block_operands.contains_key(&canonical) {
            self.deferred.get_mut().invalid = true;
            return;
        }
        self.block_operands.insert(canonical, definition);
    }

    /// Looking up an operand schedules its definition before the next write,
    /// once in this physical LLVM block. Immutable lookup lets format arguments
    /// keep using the emitter's existing value-name interface.
    pub(crate) fn block_operand(&self, canonical: &str) -> Option<String> {
        if !self.block_operands.contains_key(canonical) {
            return None;
        }
        let mut deferred = self.deferred.borrow_mut();
        let Some(label) = &self.current_label else {
            deferred.invalid = true;
            return Some(canonical.to_owned());
        };
        let request = (label.clone(), canonical.to_owned());
        if deferred.requested.insert(request.clone()) {
            deferred.pending.push(request);
        }
        Some(format!("{canonical}.{label}"))
    }

    fn flush_deferred(&mut self) {
        for (label, canonical) in self.deferred.get_mut().pending.drain(..) {
            self.parts.push(BodyPart::DeferredLoad { canonical, label });
        }
    }

    pub(crate) fn instructions(&mut self, text: &str, symbols: &[&str]) {
        self.references
            .symbols
            .extend(symbols.iter().map(|name| (*name).to_owned()));
        self.push_str(text);
    }
    pub(crate) fn type_name(
        &mut self,
        program: &crate::IrProgram,
        ty: crate::IrType,
    ) -> Result<String, BackendFailure> {
        super::emitter::llvm_type_with_references(program, ty, &mut self.references.types)
    }

    pub(crate) fn symbol(&mut self, name: impl Into<String>) {
        self.references.symbols.insert(name.into());
    }
    pub(crate) fn push_str(&mut self, text: &str) {
        self.flush_deferred();
        if let Some(BodyPart::Text(previous)) = self.parts.last_mut() {
            previous.push_str(text);
        } else {
            self.parts.push(BodyPart::Text(text.to_owned()));
        }
    }

    pub(crate) fn push(&mut self, value: char) {
        self.push_str(value.encode_utf8(&mut [0; 4]));
    }

    pub(crate) fn open_block(&mut self, label: String) {
        // A request left unwritten would land after the previous block's
        // terminator; refuse it instead of flushing it into the wrong block.
        if !self.deferred.get_mut().pending.is_empty() {
            self.deferred.get_mut().invalid = true;
        }
        let entry = self.current_label.is_none();
        self.current_label = Some(label.clone());
        self.parts.push(BodyPart::Block { label, entry });
    }

    pub(crate) fn incoming(&mut self, value: String, predecessor: IrBlockId) {
        self.parts.push(BodyPart::Incoming { value, predecessor });
    }

    /// Mark the insertion point after cleanup and block splitting, immediately
    /// before this physical block's final branch or return.
    pub(crate) fn terminator(&mut self) {
        self.flush_deferred();
        if let Some(label) = &self.current_label {
            self.parts.push(BodyPart::Terminator {
                label: label.clone(),
            });
        } else {
            self.deferred.get_mut().invalid = true;
        }
    }

    pub(crate) fn finish_ir_block(&mut self, block: IrBlockId) -> Result<(), BackendFailure> {
        self.exits
            .resize(self.exits.len().max(block.index() + 1), None);
        self.exits[block.index()] = Some(
            self.current_label
                .clone()
                .ok_or(BackendFailure::InvalidIr)?,
        );
        Ok(())
    }

    pub(crate) fn render(&self, entry_allocations: &str) -> Result<String, BackendFailure> {
        let deferred = self.deferred.borrow();
        if deferred.invalid || !deferred.pending.is_empty() {
            return Err(BackendFailure::InvalidIr);
        }
        let mut defined = BTreeSet::new();
        let mut terminators = BTreeSet::new();
        for part in &self.parts {
            match part {
                BodyPart::DeferredLoad { canonical, label } => {
                    defined.insert((label.clone(), canonical.clone()));
                }
                BodyPart::Terminator { label } => {
                    terminators.insert(label.as_str());
                }
                _ => {}
            }
        }
        // Resolve every phi before printing: predecessors can be emitted after
        // their successors, and splitting chooses their actual exit labels.
        let mut phi_loads: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for part in &self.parts {
            if let BodyPart::Incoming { value, predecessor } = part {
                let label = self.exit_label(*predecessor)?;
                if self.block_operands.contains_key(value)
                    && defined.insert((label.to_owned(), value.clone()))
                {
                    if !terminators.contains(label) {
                        return Err(BackendFailure::InvalidIr);
                    }
                    phi_loads.entry(label).or_default().push(value);
                }
            }
        }
        let mut text = String::new();
        for part in &self.parts {
            match part {
                BodyPart::Text(part) => text.push_str(part),
                BodyPart::Block { label, entry } => {
                    writeln!(text, "{label}:").map_err(|_| BackendFailure::TextEmission)?;
                    if *entry {
                        text.push_str(entry_allocations);
                    }
                }
                BodyPart::Incoming { value, predecessor } => {
                    let label = self.exit_label(*predecessor)?;
                    let value = if self.block_operands.contains_key(value) {
                        format!("{value}.{label}")
                    } else {
                        value.clone()
                    };
                    write!(text, "[ {value}, %{label} ]")
                        .map_err(|_| BackendFailure::TextEmission)?;
                }
                BodyPart::DeferredLoad { canonical, label } => {
                    self.render_block_operand(&mut text, canonical, label)?;
                }
                BodyPart::Terminator { label } => {
                    if let Some(operands) = phi_loads.remove(label.as_str()) {
                        for canonical in operands {
                            self.render_block_operand(&mut text, canonical, label)?;
                        }
                    }
                }
            }
        }
        Ok(text)
    }

    fn exit_label(&self, predecessor: IrBlockId) -> Result<&str, BackendFailure> {
        self.exits
            .get(predecessor.index())
            .and_then(Option::as_deref)
            .ok_or(BackendFailure::InvalidIr)
    }

    fn render_block_operand(
        &self,
        text: &mut String,
        canonical: &str,
        label: &str,
    ) -> Result<(), BackendFailure> {
        let definition = self
            .block_operands
            .get(canonical)
            .ok_or(BackendFailure::InvalidIr)?;
        for part in definition {
            match part {
                DeferredPart::Text(part) => text.push_str(part),
                DeferredPart::Local(name) => {
                    write!(text, "{name}.{label}").map_err(|_| BackendFailure::TextEmission)?;
                }
            }
        }
        Ok(())
    }
}

/// Dependencies recorded by the producer of a header, instruction or type.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct References {
    pub(crate) symbols: BTreeSet<String>,
    pub(crate) types: BTreeSet<String>,
    pub(crate) attributes: BTreeSet<u64>,
}

impl References {
    pub(crate) fn extend(&mut self, other: &Self) {
        self.symbols.extend(other.symbols.iter().cloned());
        self.types.extend(other.types.iter().cloned());
        self.attributes.extend(other.attributes.iter().copied());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Linkage {
    External,
    Weak,
    Private,
    Internal,
}

impl Linkage {
    pub(crate) fn is_local(self) -> bool {
        matches!(self, Self::Private | Self::Internal)
    }
    fn spelling(self) -> &'static str {
        match self {
            Self::External => "",
            Self::Weak => "weak ",
            Self::Private => "private ",
            Self::Internal => "internal ",
        }
    }
}

/// Parameter spelling and its independently retained declaration spelling.
/// The producer supplies the name separately; fragment rendering never strips
/// names out of a finished LLVM header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Parameter {
    pub(crate) ty: String,
    pub(crate) name: Option<String>,
}

impl Parameter {
    pub(crate) fn named(ty: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            ty: ty.into(),
            name: Some(name.into()),
        }
    }
    pub(crate) fn unnamed(ty: impl Into<String>) -> Self {
        Self {
            ty: ty.into(),
            name: None,
        }
    }
    pub(crate) fn render(&self) -> String {
        self.name
            .as_ref()
            .map_or_else(|| self.ty.clone(), |name| format!("{} {name}", self.ty))
    }
}

/// What closes every emitted function definition's header: its start on a
/// 64-byte boundary, then its body.
///
/// How fast a function runs can depend on where its instructions fall within
/// 64-byte lines, and any change ahead of it in the image, such as one more
/// imported library function, moves that. Aligning every definition fixes the
/// position by construction whatever precedes it. The alignment is written
/// here rather than passed to the host compiler because clang ignores
/// `-falign-functions` for LLVM input, and because this module reaches the host
/// through the driver, `--emit-llvm` consumers and link-time optimization alike
/// (design/compiler/code-alignment.md).
pub(crate) const DEFINITION_ALIGNMENT: &str = " align 64 {";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Signature {
    pub(crate) name: String,
    pub(crate) linkage: Linkage,
    /// The calling convention keyword with its trailing space, or empty for
    /// the C convention.
    pub(crate) convention: &'static str,
    pub(crate) result: String,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) suffix: String,
    pub(crate) references: References,
}

impl Signature {
    pub(crate) fn new(
        name: impl Into<String>,
        result: impl Into<String>,
        parameters: Vec<Parameter>,
    ) -> Self {
        Self {
            name: name.into(),
            linkage: Linkage::External,
            convention: "",
            result: result.into(),
            parameters,
            suffix: String::new(),
            references: References::default(),
        }
    }

    fn header(&self, definition: bool, hidden: bool) -> String {
        let parameters = self
            .parameters
            .iter()
            .map(|parameter| {
                if definition {
                    parameter.render()
                } else {
                    parameter.ty.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let visibility = if hidden {
            "hidden "
        } else if definition {
            self.linkage.spelling()
        } else {
            ""
        };
        format!(
            "{} {visibility}{}{} @{}({parameters}){}{}",
            if definition { "define" } else { "declare" },
            self.convention,
            self.result,
            self.name,
            self.suffix,
            if definition { DEFINITION_ALIGNMENT } else { "" }
        )
    }

    pub(crate) fn declaration(&self) -> Declaration {
        Declaration {
            text: self.header(false, false),
            references: self.references.clone(),
        }
    }

    pub(crate) fn define(
        mut self,
        body: FunctionBody,
        entry_allocations: &str,
    ) -> Result<Entity, BackendFailure> {
        self.references.attributes.insert(0);
        self.suffix.push_str(" #0");
        let declaration_references = self.references.clone();
        let mut references = self.references.clone();
        references.extend(&body.references);
        Ok(Entity {
            name: self.name.clone(),
            header: self.header(true, false),
            hidden_header: self.header(true, true),
            declaration: self.header(false, self.linkage.is_local()),
            declaration_references,
            body: body.render(entry_allocations)?,
            linkage: self.linkage,
            global: false,
            fragment_local: false,
            references,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Declaration {
    pub(crate) text: String,
    pub(crate) references: References,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Entity {
    pub(crate) name: String,
    pub(crate) header: String,
    pub(crate) hidden_header: String,
    pub(crate) declaration: String,
    pub(crate) declaration_references: References,
    pub(crate) body: String,
    pub(crate) linkage: Linkage,
    pub(crate) global: bool,
    /// Immutable dependency-free data replicated privately in each using
    /// fragment rather than promoted to cross-fragment linkage [STOR-7].
    pub(crate) fragment_local: bool,
    pub(crate) references: References,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ModulePart {
    Text(String),
    Type(String),
    Declaration(String),
    Entity(usize),
    Attributes(u64),
}

/// Both renderers consume the same definitions and dependency records. Parts
/// preserve whole-module order and trivia; fragment order is chosen separately.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Module {
    parts: Vec<ModulePart>,
    pub(crate) header: Vec<String>,
    pub(crate) types: BTreeMap<String, Declaration>,
    pub(crate) declarations: BTreeMap<String, Declaration>,
    pub(crate) entities: Vec<Entity>,
    pub(crate) attributes: BTreeMap<u64, String>,
}

impl Module {
    pub(crate) fn repeated_declaration(&self) -> Option<&str> {
        let mut names = BTreeSet::new();
        self.parts.iter().find_map(|part| match part {
            ModulePart::Declaration(name) if !names.insert(name.as_str()) => Some(name.as_str()),
            _ => None,
        })
    }

    /// Whether composition kept a conflicting declaration or attribute group.
    pub(crate) fn has_conflict(&self) -> bool {
        self.repeated_declaration().is_some() || self.repeated_attribute_group().is_some()
    }

    pub(crate) fn repeated_attribute_group(&self) -> Option<u64> {
        let mut groups = BTreeSet::new();
        self.parts.iter().find_map(|part| match part {
            ModulePart::Attributes(id) if !groups.insert(*id) => Some(*id),
            _ => None,
        })
    }

    fn declarations_match(&self, other: &Self, name: &str) -> bool {
        let Some(declaration) = self.declarations.get(name) else {
            return false;
        };
        if other.declarations.get(name) != Some(declaration) {
            return false;
        }
        declaration.references.attributes.iter().all(|id| {
            match (self.attributes.get(id), other.attributes.get(id)) {
                (Some(left), Some(right)) => left == right,
                // Groups may be defined after declarations. Composition gives
                // an as-yet undefined use the one recorded body; a later
                // inconsistent definition is retained as a group conflict.
                _ => true,
            }
        })
    }

    pub(crate) fn text(&mut self, text: impl Into<String>) {
        self.parts.push(ModulePart::Text(text.into()));
    }
    pub(crate) fn header(&mut self, text: String) {
        self.header.push(text.clone());
        self.text(format!("{text}\n"));
    }
    pub(crate) fn define(&mut self, entity: Entity) {
        self.parts.push(ModulePart::Entity(self.entities.len()));
        self.entities.push(entity);
    }
    pub(crate) fn declare(&mut self, signature: Signature) {
        self.register_declaration(signature.name.clone(), signature.declaration());
    }
    /// Independently emitted bodies may name the same linked function. Keep
    /// one compatible declaration, including its attributes and dependencies;
    /// conflicting records remain repeated for the module integrity check.
    fn register_declaration(&mut self, name: String, declaration: Declaration) {
        if self.declarations.get(&name) == Some(&declaration) {
            return;
        }
        self.parts.push(ModulePart::Declaration(name.clone()));
        self.declarations.insert(name, declaration);
    }
    pub(crate) fn declare_global(&mut self, name: String, ty: &str, align: u64) {
        self.register_declaration(
            name.clone(),
            Declaration {
                text: format!("@{name} = external constant {ty}, align {align}"),
                references: References::default(),
            },
        );
    }
    pub(crate) fn declare_named(&mut self, signature: Signature) {
        let parameters = signature
            .parameters
            .iter()
            .map(Parameter::render)
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!(
            "declare {} @{}({parameters}){}",
            signature.result, signature.name, signature.suffix
        );
        self.register_declaration(
            signature.name,
            Declaration {
                text,
                references: signature.references,
            },
        );
    }
    pub(crate) fn global(
        &mut self,
        name: String,
        kind: &str,
        ty: String,
        value: String,
        align: Option<u64>,
        references: References,
    ) {
        let alignment = align.map_or_else(String::new, |align| format!(", align {align}"));
        let header = format!("@{name} = private {kind} {ty} {value}{alignment}");
        let hidden_header = format!("@{name} = hidden {kind} {ty} {value}{alignment}");
        let declaration = format!(
            "@{name} = external hidden {} {ty}{alignment}",
            kind.strip_prefix("unnamed_addr ").unwrap_or(kind)
        );
        self.define(Entity {
            name,
            header,
            hidden_header,
            declaration,
            declaration_references: references.clone(),
            body: String::new(),
            linkage: Linkage::Private,
            global: true,
            fragment_local: false,
            references,
        });
    }
    /// Emit dependency-free immutable data with private storage in every
    /// emitted fragment that uses it [STOR-7]. No address identity is promised.
    pub(crate) fn fragment_local_constant(
        &mut self,
        name: String,
        ty: String,
        value: String,
        align: Option<u64>,
    ) {
        let index = self.entities.len();
        self.global(
            name,
            "unnamed_addr constant",
            ty,
            value,
            align,
            References::default(),
        );
        self.entities[index].fragment_local = true;
    }

    pub(crate) fn named_type(&mut self, name: String, body: String, references: References) {
        self.parts.push(ModulePart::Type(name.clone()));
        self.types.insert(
            name.clone(),
            Declaration {
                text: format!("%{name} = type {body}"),
                references,
            },
        );
    }
    pub(crate) fn attribute_group(&mut self, id: u64, body: String) {
        let text = format!("attributes #{id} = {{ {body} }}");
        if self.attributes.get(&id) == Some(&text) {
            return;
        }
        self.parts.push(ModulePart::Attributes(id));
        // Preserve the first meaning: a conflicting definition remains a
        // repeated part for the integrity boundary, never an overwritten body.
        self.attributes.entry(id).or_insert(text);
    }
    /// Adds `attribute` to the body of group `id`, which a module without
    /// that group does not have.
    pub(crate) fn add_group_attribute(&mut self, id: u64, attribute: &str) {
        if let Some(text) = self.attributes.get_mut(&id)
            && let Some(body) = text.strip_suffix(" }")
        {
            *text = format!("{body} {attribute} }}");
        }
    }
    pub(crate) fn append(&mut self, mut other: Self) {
        let offset = self.entities.len();
        for part in &mut other.parts {
            if let ModulePart::Entity(index) = part {
                *index += offset;
            }
        }
        // A conflict already recorded in a child module must survive merging,
        // even if its final map entry happens to match this module's entry.
        let mut seen = BTreeSet::new();
        let conflicts = other
            .parts
            .iter()
            .filter_map(|part| match part {
                ModulePart::Declaration(name) if !seen.insert(name.clone()) => Some(name.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let mut seen_groups = BTreeSet::new();
        let group_conflicts = other
            .parts
            .iter()
            .filter_map(|part| match part {
                ModulePart::Attributes(id) if !seen_groups.insert(*id) => Some(*id),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        // Resolve declaration compatibility before merging either module's
        // attribute definitions, so equal IDs cannot hide different meanings.
        let compatible = other
            .declarations
            .keys()
            .filter(|name| self.declarations_match(&other, name))
            .cloned()
            .collect::<BTreeSet<_>>();
        for part in other.parts {
            match &part {
                ModulePart::Declaration(name) => {
                    if !conflicts.contains(name) && compatible.contains(name) {
                        continue;
                    }
                    self.declarations
                        .insert(name.clone(), other.declarations[name].clone());
                }
                ModulePart::Attributes(id) => {
                    let body = &other.attributes[id];
                    if !group_conflicts.contains(id) && self.attributes.get(id) == Some(body) {
                        continue;
                    }
                    self.attributes.entry(*id).or_insert_with(|| body.clone());
                }
                _ => {}
            }
            self.parts.push(part);
        }
        self.header.extend(other.header);
        self.types.extend(other.types);
        self.entities.extend(other.entities);
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
    pub(crate) fn render(&self) -> String {
        let mut text = String::new();
        for part in &self.parts {
            match part {
                ModulePart::Text(part) => text.push_str(part),
                ModulePart::Type(name) => {
                    text.push_str(&self.types[name].text);
                    text.push('\n');
                }
                ModulePart::Declaration(name) => {
                    text.push_str(&self.declarations[name].text);
                    text.push('\n');
                }
                ModulePart::Attributes(id) => {
                    text.push_str(&self.attributes[id]);
                    text.push('\n');
                }
                ModulePart::Entity(index) => {
                    let entity = &self.entities[*index];
                    text.push_str(&entity.header);
                    text.push('\n');
                    if !entity.global {
                        text.push_str(&entity.body);
                        text.push_str("}\n");
                    }
                }
            }
        }
        text
    }
}

impl Write for FunctionBody {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.push_str(text);
        Ok(())
    }
}

/// Compiler-private cache encoding of the printing model. LLVM text is data
/// here: decoding restores the recorded graph and never infers it from text.
impl Module {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut out = ModelWriter(Vec::new());
        out.text("whitefoot emission 2");
        out.number(self.parts.len());
        for part in &self.parts {
            match part {
                ModulePart::Text(text) => {
                    out.number(0);
                    out.text(text);
                }
                ModulePart::Type(name) => {
                    out.number(1);
                    out.text(name);
                }
                ModulePart::Declaration(name) => {
                    out.number(2);
                    out.text(name);
                }
                ModulePart::Entity(index) => {
                    out.number(3);
                    out.number(*index);
                }
                ModulePart::Attributes(id) => {
                    out.number(4);
                    out.integer(*id);
                }
            }
        }
        out.strings(self.header.iter().map(String::as_str));
        for declarations in [&self.types, &self.declarations] {
            out.number(declarations.len());
            for (name, declaration) in declarations {
                out.text(name);
                out.text(&declaration.text);
                out.references(&declaration.references);
            }
        }
        out.number(self.entities.len());
        for entity in &self.entities {
            for text in [
                &entity.name,
                &entity.header,
                &entity.hidden_header,
                &entity.declaration,
                &entity.body,
            ] {
                out.text(text);
            }
            out.number(match entity.linkage {
                Linkage::External => 0,
                Linkage::Weak => 1,
                Linkage::Private => 2,
                Linkage::Internal => 3,
            });
            out.number(usize::from(entity.global));
            out.number(usize::from(entity.fragment_local));
            out.references(&entity.references);
            out.references(&entity.declaration_references);
        }
        out.number(self.attributes.len());
        for (id, text) in &self.attributes {
            out.integer(*id);
            out.text(text);
        }
        out.0
    }

    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        let mut input = ModelReader(bytes);
        if input.text()? != "whitefoot emission 2" {
            return None;
        }
        let mut module = Self::default();
        for _ in 0..input.count()? {
            module.parts.push(match input.integer()? {
                0 => ModulePart::Text(input.text()?),
                1 => ModulePart::Type(input.text()?),
                2 => ModulePart::Declaration(input.text()?),
                3 => ModulePart::Entity(input.count()?),
                4 => ModulePart::Attributes(input.integer()?),
                _ => return None,
            });
        }
        module.header = input.strings()?;
        for declarations in [&mut module.types, &mut module.declarations] {
            for _ in 0..input.count()? {
                let name = input.text()?;
                let declaration = Declaration {
                    text: input.text()?,
                    references: input.references()?,
                };
                if declarations.insert(name, declaration).is_some() {
                    return None;
                }
            }
        }
        for _ in 0..input.count()? {
            let name = input.text()?;
            let header = input.text()?;
            let hidden_header = input.text()?;
            let declaration = input.text()?;
            let body = input.text()?;
            let linkage = match input.integer()? {
                0 => Linkage::External,
                1 => Linkage::Weak,
                2 => Linkage::Private,
                3 => Linkage::Internal,
                _ => return None,
            };
            let global = match input.integer()? {
                0 => false,
                1 => true,
                _ => return None,
            };
            let fragment_local = match input.integer()? {
                0 => false,
                1 => true,
                _ => return None,
            };
            let references = input.references()?;
            let declaration_references = input.references()?;
            if fragment_local
                && (!global || linkage != Linkage::Private || references != References::default())
            {
                return None;
            }
            module.entities.push(Entity {
                name,
                header,
                hidden_header,
                declaration,
                body,
                linkage,
                global,
                fragment_local,
                references,
                declaration_references,
            });
        }
        for _ in 0..input.count()? {
            let id = input.integer()?;
            let text = input.text()?;
            if module.attributes.insert(id, text).is_some() {
                return None;
            }
        }
        if !input.0.is_empty()
            || !module.parts.iter().all(|part| match part {
                ModulePart::Text(_) => true,
                ModulePart::Type(name) => module.types.contains_key(name),
                ModulePart::Declaration(name) => module.declarations.contains_key(name),
                ModulePart::Entity(index) => *index < module.entities.len(),
                ModulePart::Attributes(id) => module.attributes.contains_key(id),
            })
        {
            return None;
        }
        Some(module)
    }
}

struct ModelWriter(Vec<u8>);
impl ModelWriter {
    fn integer(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn number(&mut self, value: usize) {
        self.integer(value as u64);
    }
    fn text(&mut self, text: &str) {
        self.number(text.len());
        self.0.extend_from_slice(text.as_bytes());
    }
    fn strings<'a>(&mut self, strings: impl ExactSizeIterator<Item = &'a str>) {
        self.number(strings.len());
        for text in strings {
            self.text(text);
        }
    }
    fn references(&mut self, references: &References) {
        self.strings(references.symbols.iter().map(String::as_str));
        self.strings(references.types.iter().map(String::as_str));
        self.number(references.attributes.len());
        for id in &references.attributes {
            self.integer(*id);
        }
    }
}

struct ModelReader<'a>(&'a [u8]);
impl ModelReader<'_> {
    fn integer(&mut self) -> Option<u64> {
        let (bytes, rest) = self.0.split_at_checked(8)?;
        self.0 = rest;
        Some(u64::from_le_bytes(bytes.try_into().ok()?))
    }
    fn count(&mut self) -> Option<usize> {
        let count = usize::try_from(self.integer()?).ok()?;
        (count <= self.0.len()).then_some(count)
    }
    fn text(&mut self) -> Option<String> {
        let size = self.count()?;
        let (text, rest) = self.0.split_at_checked(size)?;
        self.0 = rest;
        Some(std::str::from_utf8(text).ok()?.to_owned())
    }
    fn strings(&mut self) -> Option<Vec<String>> {
        (0..self.count()?).map(|_| self.text()).collect()
    }
    fn references(&mut self) -> Option<References> {
        Some(References {
            symbols: self.strings()?.into_iter().collect(),
            types: self.strings()?.into_iter().collect(),
            attributes: (0..self.count()?)
                .map(|_| self.integer())
                .collect::<Option<_>>()?,
        })
    }
}

#[cfg(test)]
mod declaration_tests {
    use super::*;

    #[test]
    fn compatible_declarations_are_idempotent_within_and_across_modules() {
        let signature = Signature::new("callee", "i32", vec![Parameter::unnamed("ptr")]);
        let mut module = Module::default();
        module.declare(signature.clone());
        module.declare(signature.clone());
        let mut other = Module::default();
        other.declare(signature);
        module.append(other);
        assert_eq!(
            module.render().matches("declare i32 @callee(ptr)").count(),
            1
        );
        assert!(module.repeated_declaration().is_none());
    }

    fn grouped_declaration(body: &str) -> Module {
        let mut module = Module::default();
        let mut signature = Signature::new("callee", "void", Vec::new());
        signature.suffix = " #1".into();
        signature.references.attributes.insert(1);
        module.declare(signature);
        module.attribute_group(1, body.into());
        module
    }

    #[test]
    fn identical_grouped_attributes_and_declarations_merge_once() {
        let mut module = grouped_declaration("noreturn");
        module.attribute_group(1, "noreturn".into());
        module.append(grouped_declaration("noreturn"));
        assert!(module.repeated_declaration().is_none());
        assert!(module.repeated_attribute_group().is_none());
        assert_eq!(
            module.render().matches("declare void @callee() #1").count(),
            1
        );
        assert_eq!(
            module.render().matches("attributes #1 = { noreturn }").count(),
            1
        );
    }

    #[test]
    fn grouped_attribute_meanings_select_declaration_compatibility() {
        let mut module = grouped_declaration("noreturn");
        module.append(grouped_declaration("nounwind"));
        assert_eq!(module.repeated_declaration(), Some("callee"));
        assert_eq!(module.repeated_attribute_group(), Some(1));
        assert_eq!(module.attributes[&1], "attributes #1 = { noreturn }");
    }

    #[test]
    fn group_conflicts_survive_nested_composition_and_cache_round_trips() {
        let mut child = Module::default();
        child.attribute_group(1, "noreturn".into());
        child.attribute_group(1, "nounwind".into());
        assert_eq!(child.repeated_attribute_group(), Some(1));
        let child = Module::decode(&child.encode()).expect("the conflict is recorded in the model");
        assert_eq!(child.repeated_attribute_group(), Some(1));
        let mut parent = Module::default();
        parent.attribute_group(1, "noreturn".into());
        parent.append(child);
        let mut ancestor = Module::default();
        ancestor.attribute_group(1, "noreturn".into());
        ancestor.append(parent);
        assert_eq!(ancestor.repeated_attribute_group(), Some(1));
        let decoded = Module::decode(&ancestor.encode()).expect("composed conflict round-trips");
        assert_eq!(decoded, ancestor);
        assert_eq!(decoded.repeated_attribute_group(), Some(1));
    }

    #[test]
    fn late_group_definitions_merge_but_conflicting_redefinitions_remain_invalid() {
        let child = grouped_declaration("noreturn");
        let mut module = Module::default();
        let mut signature = Signature::new("callee", "void", Vec::new());
        signature.suffix = " #1".into();
        signature.references.attributes.insert(1);
        module.declare(signature);
        module.append(child);
        assert!(module.repeated_declaration().is_none());
        assert!(module.repeated_attribute_group().is_none());
        module.attribute_group(1, "nounwind".into());
        assert_eq!(module.repeated_attribute_group(), Some(1));
    }

    #[test]
    fn incompatible_declarations_and_child_conflicts_remain_invalid() {
        let original = Signature::new("callee", "void", Vec::new());
        for change in ["result", "parameter", "attribute"] {
            let mut conflicting = original.clone();
            match change {
                "result" => conflicting.result = "i32".into(),
                "parameter" => conflicting.parameters.push(Parameter::unnamed("ptr")),
                "attribute" => conflicting.suffix = " noreturn".into(),
                _ => unreachable!(),
            }
            let mut child = Module::default();
            child.declare(original.clone());
            child.declare(conflicting.clone());
            assert_eq!(child.repeated_declaration(), Some("callee"));
            let mut parent = Module::default();
            parent.declare(conflicting.clone());
            parent.append(child);
            assert_eq!(parent.repeated_declaration(), Some("callee"));
            let mut parent = Module::default();
            parent.declare(original.clone());
            let mut child = Module::default();
            child.declare(conflicting);
            parent.append(child);
            assert_eq!(parent.repeated_declaration(), Some("callee"));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::{DeferredPart, FunctionBody};
    use crate::IrBlockId;

    fn block(index: usize) -> IrBlockId {
        IrBlockId::from_index(index).unwrap()
    }

    fn body_with_operand() -> FunctionBody {
        let mut body = FunctionBody::default();
        body.register_block_operand(
            "%v0".to_owned(),
            vec![
                DeferredPart::Text("  ".to_owned()),
                DeferredPart::Local("%v0".to_owned()),
                DeferredPart::Text(" = load i64, ptr %env\n".to_owned()),
            ],
        );
        body
    }

    #[test]
    fn deferred_operand_loads_once_before_its_first_regular_use() {
        let mut body = body_with_operand();
        body.open_block("bb0".to_owned());
        body.push_str("  %before = add i64 1, 2\n");
        let first = body.block_operand("%v0").unwrap();
        let repeated = body.block_operand("%v0").unwrap();
        writeln!(body, "  %sum = add i64 {first}, {repeated}").unwrap();
        let last = body.block_operand("%v0").unwrap();
        body.terminator();
        writeln!(body, "  ret i64 {last}").unwrap();
        assert_eq!(
            body.render("  %slot = alloca i64\n").unwrap(),
            "bb0:\n  %slot = alloca i64\n  %before = add i64 1, 2\n  \
             %v0.bb0 = load i64, ptr %env\n  \
             %sum = add i64 %v0.bb0, %v0.bb0\n  ret i64 %v0.bb0\n"
        );
    }

    #[test]
    fn deferred_operand_definitions_are_local_to_each_physical_branch() {
        let mut body = body_with_operand();
        body.open_block("entry".to_owned());
        body.push_str("  br i1 %condition, label %left, label %right\n");
        for label in ["left", "right"] {
            body.open_block(label.to_owned());
            let value = body.block_operand("%v0").unwrap();
            body.terminator();
            body.push_str(&format!("  ret i64 {value}\n"));
        }
        assert_eq!(
            body.render("").unwrap(),
            "entry:\n  br i1 %condition, label %left, label %right\n\
             left:\n  %v0.left = load i64, ptr %env\n  ret i64 %v0.left\n\
             right:\n  %v0.right = load i64, ptr %env\n  ret i64 %v0.right\n"
        );
    }

    #[test]
    fn phi_operands_load_at_earlier_and_later_predecessors_actual_exits() {
        let mut body = body_with_operand();
        body.open_block("bb0".to_owned());
        body.push_str("  br label %bb0.exit\n");
        body.open_block("bb0.exit".to_owned());
        body.push_str("  call void @cleanup()\n");
        body.terminator();
        body.push_str("  br label %bb1\n");
        body.finish_ir_block(block(0)).unwrap();
        body.open_block("bb1".to_owned());
        body.push_str("  %result = phi i64 ");
        body.incoming("%v0".to_owned(), block(0));
        body.push_str(", ");
        body.incoming("%v0".to_owned(), block(2));
        body.push_str("\n  ret i64 %result\n");
        body.open_block("bb2".to_owned());
        body.push_str("  br label %bb2.exit\n");
        body.open_block("bb2.exit".to_owned());
        body.terminator();
        body.push_str("  br label %bb1\n");
        body.finish_ir_block(block(2)).unwrap();
        assert_eq!(
            body.render("").unwrap(),
            "bb0:\n  br label %bb0.exit\n\
             bb0.exit:\n  call void @cleanup()\n  \
             %v0.bb0.exit = load i64, ptr %env\n  br label %bb1\n\
             bb1:\n  %result = phi i64 \
             [ %v0.bb0.exit, %bb0.exit ], [ %v0.bb2.exit, %bb2.exit ]\n  \
             ret i64 %result\n\
             bb2:\n  br label %bb2.exit\n\
             bb2.exit:\n  %v0.bb2.exit = load i64, ptr %env\n  br label %bb1\n"
        );
    }

    #[test]
    fn phi_and_regular_use_share_the_predecessors_load() {
        let mut body = body_with_operand();
        body.open_block("bb0".to_owned());
        let value = body.block_operand("%v0").unwrap();
        body.push_str(&format!("  %sum = add i64 {value}, 1\n"));
        body.terminator();
        body.push_str("  br label %bb1\n");
        body.finish_ir_block(block(0)).unwrap();
        body.open_block("bb1".to_owned());
        body.push_str("  %result = phi i64 ");
        body.incoming("%v0".to_owned(), block(0));
        body.push_str("\n  ret i64 %result\n");
        assert_eq!(
            body.render("").unwrap(),
            "bb0:\n  %v0.bb0 = load i64, ptr %env\n  \
             %sum = add i64 %v0.bb0, 1\n  br label %bb1\n\
             bb1:\n  %result = phi i64 [ %v0.bb0, %bb0 ]\n  ret i64 %result\n"
        );
    }

    #[test]
    fn phi_reuses_a_regular_load_requested_for_the_terminator() {
        let mut body = body_with_operand();
        body.open_block("bb0".to_owned());
        body.terminator();
        let value = body.block_operand("%v0").unwrap();
        body.push_str(&format!("  switch i64 {value}, label %bb1 []\n"));
        body.finish_ir_block(block(0)).unwrap();
        body.open_block("bb1".to_owned());
        body.push_str("  %result = phi i64 ");
        body.incoming("%v0".to_owned(), block(0));
        body.push_str("\n  ret i64 %result\n");
        assert_eq!(
            body.render("").unwrap(),
            "bb0:\n  %v0.bb0 = load i64, ptr %env\n  \
             switch i64 %v0.bb0, label %bb1 []\n\
             bb1:\n  %result = phi i64 [ %v0.bb0, %bb0 ]\n  ret i64 %result\n"
        );
    }

    #[test]
    fn phi_only_operand_requires_a_predecessor_terminator_marker() {
        let mut body = body_with_operand();
        body.open_block("bb0".to_owned());
        body.push_str("  br label %bb1\n");
        body.finish_ir_block(block(0)).unwrap();
        body.open_block("bb1".to_owned());
        body.push_str("  %result = phi i64 ");
        body.incoming("%v0".to_owned(), block(0));
        body.push_str("\n");
        assert!(matches!(
            body.render(""),
            Err(super::BackendFailure::InvalidIr)
        ));
    }

    #[test]
    fn operand_matching_is_exact_and_only_local_recipe_parts_are_scoped() {
        let mut body = body_with_operand();
        body.register_block_operand(
            "%v1".to_owned(),
            vec![
                DeferredPart::Text("  ".to_owned()),
                DeferredPart::Local("%v1.pointer".to_owned()),
                DeferredPart::Text(" = getelementptr i8, ptr %v0, i64 8\n  ".to_owned()),
                DeferredPart::Local("%v1".to_owned()),
                DeferredPart::Text(" = load i64, ptr ".to_owned()),
                DeferredPart::Local("%v1.pointer".to_owned()),
                DeferredPart::Text("\n".to_owned()),
            ],
        );
        body.open_block("bb0".to_owned());
        assert_eq!(body.block_operand("%v01"), None);
        assert_eq!(body.block_operand("%v0.other"), None);
        let value = body.block_operand("%v1").unwrap();
        body.push_str(&format!("  %used = add i64 {value}, %v01\n"));
        body.push_str("  br label %bb1\n");
        body.finish_ir_block(block(0)).unwrap();
        body.open_block("bb1".to_owned());
        body.push_str("  %result = phi i64 ");
        body.incoming("%v01".to_owned(), block(0));
        body.push_str("\n");
        assert_eq!(
            body.render("").unwrap(),
            "bb0:\n  %v1.pointer.bb0 = getelementptr i8, ptr %v0, i64 8\n  \
             %v1.bb0 = load i64, ptr %v1.pointer.bb0\n  \
             %used = add i64 %v1.bb0, %v01\n  br label %bb1\n\
             bb1:\n  %result = phi i64 [ %v01, %bb0 ]\n"
        );
    }

    #[test]
    fn registered_operand_lookup_without_a_physical_block_is_invalid() {
        let mut body = body_with_operand();
        assert!(body.block_operand("%v0").is_some());
        body.open_block("bb0".to_owned());
        body.push_str("  ret void\n");
        assert!(matches!(
            body.render(""),
            Err(super::BackendFailure::InvalidIr)
        ));
    }

    #[test]
    fn rendering_does_not_drop_an_unwritten_operand_request() {
        let mut body = body_with_operand();
        body.open_block("bb0".to_owned());
        assert!(body.block_operand("%v0").is_some());
        assert!(matches!(
            body.render(""),
            Err(super::BackendFailure::InvalidIr)
        ));
    }

    #[test]
    fn a_request_pending_when_the_next_block_opens_is_invalid() {
        let mut body = body_with_operand();
        body.open_block("bb0".to_owned());
        assert!(body.block_operand("%v0").is_some());
        body.open_block("bb1".to_owned());
        body.push_str("  ret void\n");
        assert!(matches!(
            body.render(""),
            Err(super::BackendFailure::InvalidIr)
        ));
    }

    #[test]
    fn registering_an_operand_after_the_first_block_is_invalid() {
        let mut body = FunctionBody::default();
        body.open_block("bb0".to_owned());
        body.register_block_operand(
            "%v0".to_owned(),
            vec![DeferredPart::Text("  nop\n".to_owned())],
        );
        body.push_str("  ret void\n");
        assert!(matches!(
            body.render(""),
            Err(super::BackendFailure::InvalidIr)
        ));
    }

    #[test]
    fn registering_the_same_operand_twice_is_invalid() {
        let mut body = body_with_operand();
        body.register_block_operand(
            "%v0".to_owned(),
            vec![DeferredPart::Text("  nop\n".to_owned())],
        );
        body.open_block("bb0".to_owned());
        body.push_str("  ret void\n");
        assert!(matches!(
            body.render(""),
            Err(super::BackendFailure::InvalidIr)
        ));
    }
}
