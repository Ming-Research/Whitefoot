# Loop permission

When the iterations of one counted loop may overlap: the accumulators, element maps, ranges, certificates and indexed reductions that make them independent.

### decision loop-elements-follow-one-affine-map
A counted loop may overlap with one scalar accumulator and with accesses at or below one proved affine-mapped element per root, or in disjoint subranges of an adjacent stride partition.
- replaced: An element family of direct subscripts only was on main; the owner changed it, because a loop that wrote one field of its own element, or passed its element to a writing helper, was denied although it wrote only element i, and Snowghost kept per-item results in separate arrays.
- rejected: an agent proposed admitting field paths and element borrows but not paths through an element's cells; the owner rejected it, because buffers owned by their items are common in a renderer, and a cell's single owner already separates them.
- replaced: Restricting writes to element maps was on main; the owner changed it, because runtime-width rows and uneven recursive subdivisions could then not use contiguous storage and ordinary helpers without runtime overlap tests.

### decision element-loops-read-root-measures
A counted loop in the affine-element or proved-range family may read the measures of the root or origin it writes, also through a helper's declared row.
- replaced: Refusing reads of a written root's measures in these families was on main; the owner changed it, because the reads touch descriptor words that no admitted iteration writes, and making the writer hoist the read withheld permission from the same independent work.

### decision apart-certificates-prove-scatter
A counted loop may carry an `apart(i, j) { use ...; }` certificate, which the range judgment checks; a holding one admits the loop's placed writes as one more family.
- rejected: an agent proposed making the certificate a checked assertion that the loop is permitted; the owner rejected it, because a holding certificate grants nothing beyond its family, and a permission refused for another reason is reported in the ledger, as for every loop.

### decision a-failing-certificate-rejects
A certificate that does not hold rejects the program.
- rejected: an agent proposed falling back to sequential execution when a certificate fails; the owner rejected it, because the writer wrote the certificate to have its proof checked, and a fallback would leave a wrong proof unreported.

### decision iterations-pass-range-references
An iteration passes its part of the storage as an ordinary range-reference argument, and iterations are separated by the shared path-overlap judgment.
- rejected: an agent proposed exclusive view descriptors, a loan mechanism for loops only, for iteration partitions; the owner rejected it, because ordinary range references carry the same partition and use the common judgment, with no other permission or lifetime mechanism.

### decision loops-reduce-into-indexed-cells
A counted loop may reduce into indexed integer or Bool cells with one fixed operation per family, also through a copied cell or a helper call, and keep its scalar accumulator apart.
- replaced: Refusing every loop in which two iterations may write one cell was on main; the owner changed it, because associativity, commutativity and identity give the sequential result per cell, with no need for distinct target cells.
- replaced: Refusing copied-cell operands and updates inside helper calls was on main; the owner changed it, because both keep the per-cell argument and recover six natural spellings that kept Snowghost below its criterion (data: `research/investigations/indexed-reductions/DESIGN.md`).

### decision unsigned-saturating-add-reduces
The operations for scalar and indexed reductions include unsigned saturating addition, with identity zero, and exclude signed saturating addition.
- replaced: Excluding all saturating addition as not associative was on main; the owner changed it, because unsigned saturation gives the sum clamped once to the type's maximum, while signed saturation can clamp before a later contribution of the opposite sign.

### decision indexed-families-take-marks-and-fields
An indexed family may be whole integer or Bool elements or a fixed integer or Bool field of record cells, each with a fixed operation or a fixed constant mark.
- replaced: Only whole elements, written as `set R[e] = R[e] op x`, was on main; the owner changed it, because a constant mark keeps touched cells by idempotence and untouched cells by having no write, and distinct fields are disjoint (data: `research/investigations/indexed-reductions/DESIGN.md`).
