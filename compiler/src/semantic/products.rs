//! Compiler-private records of retained semantic products.
//!
//! The enclosing module query validates dependencies before a product is
//! imported. Dense references in its payload address its identity table;
//! the reader replaces them with the current check's identities. Function-
//! local proof and binding identities retain their local meaning.

use std::collections::{BTreeMap, BTreeSet};

pub(crate) mod identity;

/// Identity spaces whose dense ordinals belong to one checking inventory.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum IdentityKind {
    Declaration,
    Function,
    FunctionReference,
    Nominal,
    Element,
    Constant,
    DerivedConst,
    ContractQuery,
    Module,
    Item,
    Node,
}

pub(crate) type Identity = (IdentityKind, u32);
pub(crate) type IdentityMap = BTreeMap<Identity, u32>;

/// An owned payload and every outside identity its typed writer reached.
#[derive(Default)]
pub(crate) struct Writer {
    pub(crate) bytes: Vec<u8>,
    pub(crate) identities: BTreeSet<Identity>,
    positions: Vec<(usize, Identity)>,
}

impl Writer {
    pub(crate) fn identity(&mut self, kind: IdentityKind, value: u32) {
        self.identities.insert((kind, value));
        self.positions.push((self.bytes.len(), (kind, value)));
        value.write(self);
    }

    /// Canonical input bytes with every inventory reference spelled by its
    /// structural identity. No derived Debug representation participates.
    pub(crate) fn canonical(
        &self,
        mut name: impl FnMut(Identity) -> Option<Vec<u8>>,
    ) -> Option<Vec<u8>> {
        let mut out = Writer::default();
        let mut at = 0;
        for &(position, identity) in &self.positions {
            u8::write_slice(self.bytes.get(at..position)?, &mut out);
            name(identity)?.write(&mut out);
            at = position.checked_add(4)?;
        }
        u8::write_slice(self.bytes.get(at..)?, &mut out);
        Some(out.bytes)
    }
}

type OriginLookup<'a> = dyn Fn(&crate::NodePath, u32, u32) -> Option<crate::SourceOrigin> + 'a;

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    identities: &'a IdentityMap,
    origins: Option<&'a OriginLookup<'a>>,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], identities: &'a IdentityMap) -> Self {
        Self {
            bytes,
            identities,
            origins: None,
        }
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (value, remaining) = self.bytes.split_at_checked(count)?;
        self.bytes = remaining;
        Some(value)
    }

    pub(crate) fn identity(&mut self, kind: IdentityKind) -> Option<u32> {
        let previous = u32::read(self)?;
        self.identities.get(&(kind, previous)).copied()
    }

    pub(crate) fn finished(&self) -> bool {
        self.bytes.is_empty()
    }

    pub(crate) fn with_origins(mut self, origins: &'a OriginLookup<'a>) -> Self {
        self.origins = Some(origins);
        self
    }
}

/// One concrete Rust value in a version-private product. The compiler's
/// executable identity already scopes the enclosing cache record.
pub(crate) trait Record: Sized {
    fn write(&self, writer: &mut Writer);
    fn read(reader: &mut Reader<'_>) -> Option<Self>;

    fn write_slice(values: &[Self], writer: &mut Writer) {
        values.len().write(writer);
        for value in values {
            value.write(writer);
        }
    }

    fn read_vec(reader: &mut Reader<'_>) -> Option<Vec<Self>> {
        let count = usize::read(reader)?;
        (0..count).map(|_| Self::read(reader)).collect()
    }
}

macro_rules! integers {
    ($($integer:ty),* $(,)?) => {$ (
        impl Record for $integer {
            fn write(&self, writer: &mut Writer) {
                writer.bytes.extend_from_slice(&self.to_le_bytes());
            }

            fn read(reader: &mut Reader<'_>) -> Option<Self> {
                Some(Self::from_le_bytes(reader.take(size_of::<Self>())?.try_into().ok()?))
            }
        }
    )*};
}

integers!(u16, u32, u64, u128, i8, i16, i32, i64, i128);

impl Record for u8 {
    fn write(&self, writer: &mut Writer) {
        writer.bytes.push(*self);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some(*reader.take(1)?.first()?)
    }
    fn write_slice(values: &[Self], writer: &mut Writer) {
        values.len().write(writer);
        writer.bytes.extend_from_slice(values);
    }
    fn read_vec(reader: &mut Reader<'_>) -> Option<Vec<Self>> {
        let count = usize::read(reader)?;
        Some(reader.take(count)?.to_vec())
    }
}

impl Record for usize {
    fn write(&self, writer: &mut Writer) {
        (*self as u64).write(writer);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        u64::read(reader)?.try_into().ok()
    }
}

impl Record for bool {
    fn write(&self, writer: &mut Writer) {
        u8::from(*self).write(writer);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        match u8::read(reader)? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
}

impl<T: Record> Record for Vec<T> {
    fn write(&self, writer: &mut Writer) {
        T::write_slice(self, writer);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        T::read_vec(reader)
    }
}

impl<T: Record> Record for Option<T> {
    fn write(&self, writer: &mut Writer) {
        self.is_some().write(writer);
        if let Some(value) = self {
            value.write(writer);
        }
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        if bool::read(reader)? {
            Some(Some(T::read(reader)?))
        } else {
            Some(None)
        }
    }
}

impl<T: Record> Record for Box<T> {
    fn write(&self, writer: &mut Writer) {
        (**self).write(writer);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        T::read(reader).map(Box::new)
    }
}

impl<T: Record> Record for Box<[T]> {
    fn write(&self, writer: &mut Writer) {
        T::write_slice(self, writer);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Vec::<T>::read(reader).map(Vec::into_boxed_slice)
    }
}

impl<T: Record, const N: usize> Record for [T; N] {
    fn write(&self, writer: &mut Writer) {
        for value in self {
            value.write(writer);
        }
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        (0..N)
            .map(|_| T::read(reader))
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .ok()
    }
}

impl Record for String {
    fn write(&self, writer: &mut Writer) {
        self.len().write(writer);
        writer.bytes.extend_from_slice(self.as_bytes());
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let length = usize::read(reader)?;
        std::str::from_utf8(reader.take(length)?)
            .ok()
            .map(str::to_owned)
    }
}

impl Record for &'static str {
    fn write(&self, writer: &mut Writer) {
        self.len().write(writer);
        writer.bytes.extend_from_slice(self.as_bytes());
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        super::check::retained_reference_event(&String::read(reader)?)
    }
}

impl<A: Record, B: Record> Record for (A, B) {
    fn write(&self, writer: &mut Writer) {
        self.0.write(writer);
        self.1.write(writer);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some((A::read(reader)?, B::read(reader)?))
    }
}

impl<A: Record, B: Record, C: Record> Record for (A, B, C) {
    fn write(&self, writer: &mut Writer) {
        self.0.write(writer);
        self.1.write(writer);
        self.2.write(writer);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some((A::read(reader)?, B::read(reader)?, C::read(reader)?))
    }
}

impl Record for crate::NodePath {
    fn write(&self, writer: &mut Writer) {
        self.components.len().write(writer);
        if let Some((item, path)) = self.components.split_first() {
            writer.identity(IdentityKind::Item, *item);
            for component in path {
                component.write(writer);
            }
        }
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let count = usize::read(reader)?;
        let mut components = Vec::new();
        if count > 0 {
            components.push(reader.identity(IdentityKind::Item)?);
            for _ in 1..count {
                components.push(u32::read(reader)?);
            }
        }
        Some(Self { components })
    }
}

impl Record for crate::DeclarationId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Declaration, self.index() as u32);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Self::from_index(reader.identity(IdentityKind::Declaration)? as usize)
    }
}

impl Record for crate::ModuleId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Module, self.index() as u32);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Self::from_index(reader.identity(IdentityKind::Module)? as usize)
    }
}

impl Record for crate::SourceOrigin {
    fn write(&self, writer: &mut Writer) {
        self.node().write(writer);
        self.role_ordinal().write(writer);
        self.subtoken_ordinal().write(writer);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let node = crate::NodePath::read(reader)?;
        let role = u32::read(reader)?;
        let subtoken = u32::read(reader)?;
        reader.origins?(&node, role, subtoken)
    }
}

/// Explicit field lists are exhaustive in both directions. Adding a field
/// requires changing the product; silently omitting it is a compile error.
macro_rules! record_struct {
    ($name:ty { $($field:ident),* $(,)? }) => {
        impl $crate::semantic::products::Record for $name {
            fn write(&self, writer: &mut $crate::semantic::products::Writer) {
                let Self { $($field),* } = self;
                $($field.write(writer);)*
            }
            fn read(reader: &mut $crate::semantic::products::Reader<'_>) -> Option<Self> {
                Some(Self { $($field: $crate::semantic::products::Record::read(reader)?),* })
            }
        }
    };
}

macro_rules! record_enum {
    ($name:ty { $( $tag:literal => $variant:ident $( ( $($tuple:ident),* ) )? $( { $($field:ident $( : $kind:ident )?),* } )? ),* $(,)? }) => {
        impl $crate::semantic::products::Record for $name {
            fn write(&self, writer: &mut $crate::semantic::products::Writer) {
                match self {
                    $(Self::$variant $( ( $($tuple),* ) )? $( { $($field),* } )? => {
                        ($tag as u32).write(writer);
                        $( $($tuple.write(writer);)* )?
                        $( $($crate::semantic::products::record_enum!(@write $field, writer $(, $kind)?);)* )?
                    }),*
                }
            }
            fn read(reader: &mut $crate::semantic::products::Reader<'_>) -> Option<Self> {
                Some(match u32::read(reader)? {
                    $($tag => Self::$variant
                        $( ( $($crate::semantic::products::record_enum!(@read $tuple, reader)),* ) )?
                        $( { $($field: $crate::semantic::products::record_enum!(@read_field reader $(, $kind)?)),* } )?),*,
                    _ => return None,
                })
            }
        }
    };
    (@read $field:ident, $reader:ident) => { $crate::semantic::products::Record::read($reader)? };
    (@write $field:ident, $writer:ident) => { $field.write($writer) };
    (@write $field:ident, $writer:ident, $kind:ident) => {
        $writer.identity($crate::semantic::products::IdentityKind::$kind, *$field)
    };
    (@read_field $reader:ident) => { $crate::semantic::products::Record::read($reader)? };
    (@read_field $reader:ident, $kind:ident) => {
        $reader.identity($crate::semantic::products::IdentityKind::$kind)?
    };
}

macro_rules! record_tuple {
    ($name:ty, $($index:tt),+ $(,)?) => {
        impl $crate::semantic::products::Record for $name {
            fn write(&self, writer: &mut $crate::semantic::products::Writer) {
                $(self.$index.write(writer);)+
            }
            fn read(reader: &mut $crate::semantic::products::Reader<'_>) -> Option<Self> {
                Some(Self($($crate::semantic::products::record_tuple!(@read $index, reader)),+))
            }
        }
    };
    (@read $index:tt, $reader:ident) => { $crate::semantic::products::Record::read($reader)? };
}

pub(crate) use {record_enum, record_struct, record_tuple};

record_enum!(crate::semantic::SemanticRule {
    0 => Form5,
    1 => Form7,
    2 => Gram6,
    3 => Give1,
    4 => Gram8,
    5 => Gram10,
    6 => Gram11,
    7 => Type2,
    8 => Mod5,
    9 => Mod6,
    10 => Type5,
    11 => Type6,
    12 => Type9,
    13 => Type10,
    14 => Type7,
    15 => Set1,
    16 => Const1,
    17 => Const2,
    18 => Own1,
    19 => Ref1,
    20 => Ref2,
    21 => Ref3,
    22 => Ref4,
    23 => Own11,
    24 => Liv1,
    25 => Prov6,
    26 => Win3,
    27 => Stor8,
    28 => Op1,
    29 => Op2,
    30 => Op4,
    31 => Op5,
    32 => Op6,
    33 => Op9,
    34 => Op10,
    35 => Op11,
    36 => Op12,
    37 => Op14,
    38 => Fn1,
    39 => Fn2,
    40 => Fn3,
    41 => Fn4,
    42 => Fn5,
    43 => Fn6,
    44 => Fn8,
    45 => Fn9,
    46 => Fn10,
    47 => Call4,
    48 => Eff1,
    49 => Eff2,
    50 => Eff5,
    51 => Err2,
    52 => Err3,
    53 => Ent2,
    54 => Msr3,
    55 => Call6,
    56 => Inv1,
    57 => Prf1,
});

impl Record for crate::syntax::NodeId {
    fn write(&self, writer: &mut Writer) {
        writer.identity(IdentityKind::Node, self.index() as u32);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Self::from_index(reader.identity(IdentityKind::Node)? as usize)
    }
}

record_enum!(IdentityKind {
    0 => Declaration,
    1 => Function,
    2 => FunctionReference,
    3 => Nominal,
    4 => Element,
    5 => Constant,
    6 => DerivedConst,
    7 => ContractQuery,
    8 => Module,
    9 => Item,
    10 => Node,
});

impl Record for crate::BuiltinPreludeId {
    fn write(&self, writer: &mut Writer) {
        self.ordinal().write(writer);
    }
    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Self::from_ordinal(u8::read(reader)?)
    }
}
