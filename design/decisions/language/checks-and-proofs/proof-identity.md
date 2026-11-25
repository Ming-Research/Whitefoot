# Proof identity

When two spellings of a place are one term for the checker.

### decision exact-reference-path-identity
A place written through an exact reference variable, one no `set` rebinds and whose path is exact, is identified as a term and a goal datum by the one path that variable names.
- replaced: identifying every place by its spelling alone was on main; the owner changed it, because a guard read through a reference variable proved nothing about a call through it once growth or an append loop killed the equalities linking the spellings.
- rejected: an agent proposed judging exactness at each read, whatever the rebindings; the owner rejected it, because a variable rebound in a loop would name its target by path before the loop and by spelling at the header, so a header invariant read through it loses its entry proof.
- rejected: an agent proposed entry referents for rebound parameters and `atomic` binders, with rebinding kills; the owner rejected it, because it adds identities and kills used only by paths rooted at a rebound variable, which keeping such paths apart already makes sound.

### decision page-selector-identity
Direct segment and page selections compare their selectors as index steps, while a borrowed page keeps its formation identity and publishes its length's equality to the direct page length.
- rejected: an agent proposed fresh identities for direct page reads, or no equality at formation; the owner rejected it, because a direct page selection reads the current extent each time, with writes to the owner's length killing its facts.
- rejected: an agent proposed identifying borrowed extents across formations; the owner rejected it, because a borrowed page captures its extent at formation and keeps it across a later append.

### decision range-alias-through-source
A range-reference variable bound by `let` to another one, neither rebound, is identified through its source, so `q^.len`, `q^[i]` and `q^[a..b]` are the source's terms.
- replaced: identifying every range-reference variable through itself was on main; the owner changed it, because the writer's alias `let q = r;`, the repair TYPE-7 names, then lost every fact about its source.
- rejected: an agent proposed publishing only `q^.len == r^.len` at the alias's `let`; the owner rejected it, because it carries the length alone, so a fact about a place below the run still does not reach a read through the alias.