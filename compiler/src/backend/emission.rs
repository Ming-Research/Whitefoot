//! Target emission structure shared by whole-module and fragment rendering.
//!
//! This is a printing model, not an optimization IR. Instructions remain text;
//! block identity and references which other emission consumers need do not.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write};

use crate::IrBlockId;

use super::emitter::BackendFailure;

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
}

/// A function's emitted blocks and deferred incoming edges. Recording a label
/// when its block is opened gives an IR predecessor its actual target exit.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FunctionBody {
    parts: Vec<BodyPart>,
    current_label: Option<String>,
    exits: Vec<Option<String>>,
    pub(crate) references: References,
}

impl FunctionBody {
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
        let entry = self.current_label.is_none();
        self.current_label = Some(label.clone());
        self.parts.push(BodyPart::Block { label, entry });
    }

    pub(crate) fn incoming(&mut self, value: String, predecessor: IrBlockId) {
        self.parts.push(BodyPart::Incoming { value, predecessor });
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
                    let label = self
                        .exits
                        .get(predecessor.index())
                        .and_then(Option::as_ref)
                        .ok_or(BackendFailure::InvalidIr)?;
                    write!(text, "[ {value}, %{label} ]")
                        .map_err(|_| BackendFailure::TextEmission)?;
                }
            }
        }
        Ok(text)
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Signature {
    pub(crate) name: String,
    pub(crate) linkage: Linkage,
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
            "{} {visibility}{} @{}({parameters}){}{}",
            if definition { "define" } else { "declare" },
            self.result,
            self.name,
            self.suffix,
            if definition { " {" } else { "" }
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
        self.parts
            .push(ModulePart::Declaration(signature.name.clone()));
        self.declarations
            .insert(signature.name.clone(), signature.declaration());
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
        self.parts
            .push(ModulePart::Declaration(signature.name.clone()));
        self.declarations.insert(
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
            references,
        });
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
        self.parts.push(ModulePart::Attributes(id));
        self.attributes
            .insert(id, format!("attributes #{id} = {{ {body} }}"));
    }
    pub(crate) fn append(&mut self, mut other: Self) {
        let offset = self.entities.len();
        for part in &mut other.parts {
            if let ModulePart::Entity(index) = part {
                *index += offset;
            }
        }
        self.parts.extend(other.parts);
        self.header.extend(other.header);
        self.types.extend(other.types);
        self.declarations.extend(other.declarations);
        self.entities.extend(other.entities);
        self.attributes.extend(other.attributes);
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
        out.text("whitefoot emission 1");
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
        if input.text()? != "whitefoot emission 1" {
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
            let references = input.references()?;
            let declaration_references = input.references()?;
            module.entities.push(Entity {
                name,
                header,
                hidden_header,
                declaration,
                body,
                linkage,
                global,
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
