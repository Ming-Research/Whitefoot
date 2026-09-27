//! Typed private encoding of the retained values defined by the parent.

use super::*;
use crate::semantic::products::{IdentityKind, Reader, Record, Writer, record_enum, record_struct};

impl Record for CaptureId {
    fn write(&self, writer: &mut Writer) {
        match self {
            Self::Source(node) => {
                0_u32.write(writer);
                writer.identity(IdentityKind::Node, *node);
            }
            Self::LoopHeader {
                loop_id,
                holder,
                path,
                position,
            } => {
                1_u32.write(writer);
                loop_id.write(writer);
                holder.write(writer);
                path.write(writer);
                position.write(writer);
            }
            Self::ValueDetermined => 2_u32.write(writer),
            Self::SpellingDetermined => 3_u32.write(writer),
            Self::Unknown => 4_u32.write(writer),
        }
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some(match u32::read(reader)? {
            0 => Self::Source(reader.identity(IdentityKind::Node)?),
            1 => Self::LoopHeader {
                loop_id: Record::read(reader)?,
                holder: Record::read(reader)?,
                path: Record::read(reader)?,
                position: Record::read(reader)?,
            },
            2 => Self::ValueDetermined,
            3 => Self::SpellingDetermined,
            4 => Self::Unknown,
            _ => return None,
        })
    }
}

record_enum!(CapturedTerm {
    0 => Literal(f0),
    1 => Binding(f0),
    2 => Superseded(f0),
    3 => Const(f0),
    4 => Opaque,
});

record_struct!(CapturedValue { capture, term });

record_struct!(CapturedRange { start, end });

record_enum!(WindowPart {
    0 => Next,
    1 => Last,
    2 => Filled,
    3 => Free,
});

record_enum!(PlaceRoot {
    0 => Binding(f0),
    1 => Constant(f0),
});

record_enum!(PlaceStep {
    0 => Descendant(f0),
    1 => Field(f0),
    2 => Deref,
    3 => Payload { variant, field },
    4 => Index(f0),
    5 => Range(f0),
    6 => Part(f0),
    7 => Measure(f0),
});

record_struct!(DescendantTarget {
    loop_id,
    holder,
    ty,
    range,
    readonly
});

record_struct!(ResolvedPlace { root, path });
