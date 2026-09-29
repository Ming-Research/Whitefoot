//! The three compiler-owned storage nominals [TYPE-9], the cell identity they
//! share a reader with, and their [TYPE-6] lookup classes.
//!
//! x1 makes all four of them prelude records. [TYPE-2] and [PRE-1] declare
//! `Array`, `Slots`, `Ring` and `Box` as opaque structs, parsed like every
//! other prelude record, so each contributes its nominal-type entry, its
//! refused constructor entry and its fields through the ordinary declaration
//! path, and a source declaration of the same spelling is the ordinary PRE-1
//! collision.
//!
//! What survives in this module is their *identity*. A written `Array<T, n>`
//! or `Box<T>` names one compiler-owned shape rather than a source struct,
//! because only the identity carries what a struct body cannot state: the
//! element storage, the omitted-capacity form and the [MSR-1] measure row.
//! Resolution maps each of those four declarations onto its
//! [`ContainerNominalId`] through [`container_nominal_id`], and every later
//! stage reads the shapes and the cell exactly where it read them before.
//!
//! [BLK-0]'s kernel declaration domain is gone in v0.60: the construction
//! functions [OP-13] and the window operations [OP-10] are ordinary [PRE-1]
//! records parsed by the same grammar as source declarations, so this module
//! carries no operation table any more.

use super::DeclarationClass;

/// Dense identity of one compiler-owned container: a [TYPE-9] storage nominal
/// or the [TYPE-2] cell.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContainerNominalId(u8);

impl ContainerNominalId {
    pub(crate) const fn new(ordinal: u8) -> Self {
        Self(ordinal)
    }

    /// Returns the zero-based index into [`CONTAINER_NOMINALS`], or
    /// [`CELL_NOMINAL_ID`]'s ordinal, which is one past its end.
    #[must_use]
    pub const fn ordinal(self) -> u8 {
        self.0
    }
}

/// One compiler-owned nominal spelling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainerNominal {
    /// Exact TYPEID spelling.
    pub spelling: &'static str,
    /// Which of the four this row is.
    pub shape: ContainerShape,
}

/// The three [TYPE-9] storage shapes and the [TYPE-2] cell.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContainerShape {
    /// `Array<T, N>` and `Array<T>`: every slot always holds a value and
    /// `a.len == a.cap` [WIN-1].
    Array,
    /// `Slots<T, N>` and `Slots<T>`: a window whose filled prefix is `r.len`
    /// [WIN-1].
    Slots,
    /// `Ring<T, N>` and `Ring<T>`: a window whose logical origin is `r.head`
    /// [WIN-1, MSR-1].
    Ring,
    /// `Segments<T>`: a run of segments of T whose boundaries are fixed at
    /// construction; it exists only as `Box` content [TYPE-9].
    Segments,
    /// `Box<T>`: one heap object of any nameable T, carrying no brand and no
    /// measure at all, a cell being never empty [TYPE-9]. Its declaration is
    /// the prelude's opaque struct [TYPE-2, PRE-1], not a row below.
    Box,
}

/// The three storage nominals, in [TYPE-9] order. The cell is not one of them:
/// it is declared by [PRE-1] and read through [`CELL_NOMINAL`].
pub const CONTAINER_NOMINALS: [ContainerNominal; 4] = [
    ContainerNominal {
        spelling: "Array",
        shape: ContainerShape::Array,
    },
    ContainerNominal {
        spelling: "Slots",
        shape: ContainerShape::Slots,
    },
    ContainerNominal {
        spelling: "Ring",
        shape: ContainerShape::Ring,
    },
    ContainerNominal {
        spelling: "Segments",
        shape: ContainerShape::Segments,
    },
];

/// The cell's compiler-owned identity [TYPE-2, TYPE-9].
///
/// `Box` is declared by [PRE-1] rather than by a row of [`CONTAINER_NOMINALS`],
/// so nothing iterating that table offers it a nominal-type or constructor
/// entry of its own; what this record supplies is the identity every stage
/// after resolution reads a written `Box` through.
pub const CELL_NOMINAL: ContainerNominal = ContainerNominal {
    spelling: "Box",
    shape: ContainerShape::Box,
};

/// The identity resolution gives a use that names [PRE-1]'s cell declaration.
pub const CELL_NOMINAL_ID: ContainerNominalId =
    ContainerNominalId::new(CONTAINER_NOMINALS.len() as u8);

/// The compiler-owned identity a use of [PRE-1]'s opaque storage declarations
/// resolves to [TYPE-2, TYPE-9].
///
/// The four are ordinary prelude declarations now, so resolution reaches them
/// through the ordinary declaration path; what this mapping supplies is the
/// identity every later stage reads a written `Array`, `Slots`, `Ring` or
/// `Box` through, because only the identity carries the element storage, the
/// omitted-capacity form and the [MSR-1] measure row no struct body states.
#[must_use]
pub fn container_nominal_id(spelling: &str) -> Option<ContainerNominalId> {
    if spelling == CELL_NOMINAL.spelling {
        return Some(CELL_NOMINAL_ID);
    }
    CONTAINER_NOMINALS
        .iter()
        .position(|nominal| nominal.spelling == spelling)
        .and_then(|ordinal| u8::try_from(ordinal).ok())
        .map(ContainerNominalId::new)
}

/// The nominal record one resolved container target names: a [TYPE-9] storage
/// shape, or the [TYPE-2] cell at the ordinal one past them.
#[must_use]
pub fn container_nominal(id: ContainerNominalId) -> Option<&'static ContainerNominal> {
    if id.ordinal() == CELL_NOMINAL_ID.ordinal() {
        return Some(&CELL_NOMINAL);
    }
    CONTAINER_NOMINALS.get(usize::from(id.ordinal()))
}

/// The lookup classes of every storage nominal: one entry of the nominal-type
/// TYPEID domain and one of the constructor TYPEID domain [TYPE-6, TYPE-9],
/// exactly as a source `struct_decl` contributes both.
pub const CONTAINER_NOMINAL_CLASSES: [DeclarationClass; 2] = [
    DeclarationClass::NominalType,
    DeclarationClass::StructConstructor,
];

/// The nominal-type class of a storage nominal, which every `type` position
/// admits [TYPE-6].
pub const CONTAINER_NOMINAL_CLASS: DeclarationClass = DeclarationClass::NominalType;

#[cfg(test)]
mod tests {
    use super::{CELL_NOMINAL, CELL_NOMINAL_ID, CONTAINER_NOMINALS, container_nominal};

    /// [FORM-3]: every storage nominal is TYPEID-shaped, so the three rows
    /// occupy the nominal-type and constructor domains and take no lexical
    /// IDENT spelling away from a writer's declarations.
    #[test]
    fn every_storage_nominal_is_a_typeid() {
        for nominal in CONTAINER_NOMINALS {
            assert!(
                crate::syntax::terminal::is_type_identifier(nominal.spelling.as_bytes()),
                "{} is not a TYPEID spelling",
                nominal.spelling
            );
        }
    }

    /// [TYPE-6]: spellings are unique within each domain.
    #[test]
    fn storage_nominal_spellings_are_unique() {
        let mut nominals: Vec<_> = CONTAINER_NOMINALS
            .iter()
            .map(|nominal| nominal.spelling)
            .collect();
        nominals.sort_unstable();
        let count = nominals.len();
        nominals.dedup();
        assert_eq!(nominals.len(), count);
    }

    /// [TYPE-9]: the three nominals are exactly the shapes the rule names,
    /// read out of its own text rather than pinned by hand. The sentence that
    /// names them also states the count, so a shape entering or leaving the
    /// table is read here and not counted by hand.
    #[test]
    fn storage_nominals_match_the_active_specification() {
        let body = crate::ACTIVE_KERNEL_SPEC_TEXT
            .split_once("[TYPE-9] Three storage shapes, two placements each, and one cell.")
            .expect("exact TYPE-9 opening")
            .1
            .split_once("\n\n")
            .expect("exact TYPE-9 body")
            .0;
        for nominal in CONTAINER_NOMINALS {
            assert!(
                body.contains(&format!("`{}<", nominal.spelling)),
                "TYPE-9 does not name {}",
                nominal.spelling
            );
        }
        assert_eq!(CONTAINER_NOMINALS.len(), 3);
    }

    /// [TYPE-2, PRE-1]: the cell is declared by the prelude, so it takes no
    /// row of the storage table, and its identity is readable at the ordinal
    /// one past that table's end.
    #[test]
    fn the_cell_is_outside_the_storage_table_and_still_readable() {
        assert!(
            !CONTAINER_NOMINALS
                .iter()
                .any(|nominal| nominal.spelling == CELL_NOMINAL.spelling)
        );
        assert_eq!(container_nominal(CELL_NOMINAL_ID), Some(&CELL_NOMINAL));
        assert!(crate::ACTIVE_KERNEL_SPEC_TEXT.contains(
            "`Box<T>` is the prelude's opaque struct `opaque nocopy struct Box<T> { inner: T; }`"
        ));
    }

    /// x1 [TYPE-2, PRE-1]: the three storage shapes are prelude declarations
    /// too, so every one of the four spellings resolves to its compiler-owned
    /// identity through the one mapping resolution reads.
    #[test]
    fn every_storage_spelling_maps_to_its_container_identity() {
        for (ordinal, nominal) in CONTAINER_NOMINALS.iter().enumerate() {
            assert_eq!(
                crate::container_nominal_id(nominal.spelling).map(|id| usize::from(id.ordinal())),
                Some(ordinal),
                "{} resolves to its own storage identity",
                nominal.spelling
            );
        }
        assert_eq!(
            crate::container_nominal_id(CELL_NOMINAL.spelling),
            Some(CELL_NOMINAL_ID)
        );
        assert_eq!(crate::container_nominal_id("Inputs"), None);
    }
}
