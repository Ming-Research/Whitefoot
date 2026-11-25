# Enum layout

How enums are laid out in memory and moved: tag widths, the union of variant views for multi-payload enums, and the one-bit and count-sized tags of tag-only enums.

### decision a-payload-enum-tag-is-sized-by-its-variant-count
Every payload enum uses an unsigned tag of the smallest of 8, 16 and 32 bits that holds its variant ordinals, first in the value, with fields in declaration order.
- replaced: A fixed 32-bit tag was on main; the owner changed it, because count-sized tags make stored enums smaller within OP-9's unchanged ceilings, and Halo's instruction Cell shrank from 12 to 8 bytes.

### decision a-union-moves-its-tag-with-the-common-padding
A union's carrier, initialization and bounded transfer join its tag with the leading padding common to every variant into the largest 1-, 2- or 4-byte prefix before any payload word.
- rejected: an agent proposed moving the tag-to-payload gap as its own three-byte interval; the owner rejected it, because the extra moves slowed Halo by 5 to 11 percent, while with the prefix it measured as before.
- rejected: an agent proposed widening the physical tag; the owner rejected it, because the common padding needs no access of its own.

### decision a-multi-payload-enum-is-a-union-of-variant-views
An enum with at least two payload variants whose product form does not return in registers is laid out as a union of per-variant views, sized and aligned for the largest variant.
- replaced: The product layout for every enum was on main; the owner changed it, because it multiplies storage by the number of payload variants, 168 against 40 bytes for Snowghost's `Component` (data: `research/investigations/enum-union-layout/DESIGN.md`).
- rejected: an agent proposed one payload area at one offset after the tag, as a C union places it; the owner rejected it, because aligning every payload to the most aligned one can exceed the product layout and the OP-9 ceiling, so accepted programs would fail target qualification.
- rejected: an agent proposed union layout with a word carrier for enums that return in registers; the owner rejected it, because it saves at most 8 bytes on such values in the maintained programs, while packing leaves into integer words.
- rejected: an agent proposed union layout with a destination for every multi-payload enum; the owner rejected it, because it would send results now returned in registers through memory, undoing the measured gain of register results.
- rejected: an agent proposed boxing large variants or restructuring interfaces in the writer's source; the owner rejected it, because under the product layout boxing still sums one pointer per variant, and one library restructure cannot serve many different payloads.

### decision a-union-laid-out-enum-stays-in-memory
A union-laid-out enum is memory-only: fields are read through variant views, calls pass it by address and return it through a destination, and whole moves go by bounded intervals.
- rejected: an agent proposed a first-class byte or word carrier for union values; the owner rejected it, because LLVM has no union type, and a word can carry pointers, padding or one-bit leaves through a type that does not keep them.

### decision tag-only-enums-are-one-bit-or-count-sized
Bool and tag-only enums with at most two variants lower to one bit, and larger tag-only enums to the smallest of 8, 16 and 32 bits that holds their ordinals.
- rejected: an agent proposed word-sized tags for Bool and other two-state enums; the owner rejected it, because they impose wide loop-carried state where the same choice fits in one bit.
- replaced: A 32-bit tag for every larger tag-only enum was on main; the owner changed it, because count-sized tags make stored enums smaller within OP-9's unchanged ceilings.