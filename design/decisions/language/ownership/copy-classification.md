# Copy classification

The copy and drop capabilities: how a type gets them, the modifiers that remove them, and how a use of a value is spelled.

### decision copy-and-drop-capabilities
A type has two capabilities, copy and drop; copy, affine and linear are prose names defined over them, and a reference has no class.
- replaced: Three classes as the primitive notion, with a keyword for each, was on main; the owner changed it, because what a generic bound asks for and what a declaration modifier removes are the two capabilities, so the keywords name capabilities.
- rejected: an agent proposed giving references a class so that one can be copied, stored or returned; the owner rejected it, because a reference names a path and owns no storage, and a stored one would outlive the path it names.

### decision copy-is-structural
A type has copy or drop exactly when every part it owns has it and its declaration does not remove it, so a struct of copy fields and a tag-only enum are copy.
- replaced: Copy only for primitives and tag-only enums, with every other owned composite affine, was on main; the owner changed it, because small value structs must be copyable, the rule was only a starting simplification, and structural copy hides no cost.
- rejected: an agent proposed making every enum affine whatever its payload, Bool included; the owner rejected it, because it bought no safety and forced integer-flag workarounds that cost a measured 1.6 to 1.8 times on scanner kernels.

### decision nocopy-and-nodrop-modifiers
A declaration removes a capability with `nocopy` or `nodrop`, at most one of them, and `nodrop` removes copy together with drop.
- replaced: The modifier `linear` was on main; the owner changed it, because it is a class name that shares no root with the bound asking for the capability it removes, and nothing removed copy.
- rejected: an agent proposed the words `discard` and `nodiscard`; the owner rejected it, because `drop` parallels Rust, whose readers already know that word for the end of a value, while `discard` would be a second word for the same event.
- rejected: an agent proposed the words `release` and `norelease`; the owner rejected it, because `norelease` reads as a value that is never released, the opposite of a value the program must release itself.
- rejected: an agent proposed the word `forget`; the owner rejected it, because in Rust it means leaking a value without running its release.
- rejected: an agent proposed requiring a payload that is not copy before a declaration may remove drop; the owner rejected it, because a consumption token would then need a dummy resource field, while removing a capability does not depend on structure.

### decision move-on-copy-is-an-error
`move` written on a copy value is a hard error, and a copy value is used bare.
- rejected: an agent proposed admitting `move` on a copy value as a redundant spelling of the bare use; the owner rejected it, because `move` then marks in the source exactly the values that cannot be copied, and one meaning keeps one spelling.

### decision generic-move-checked-once
A generic body's consuming spelling is checked once against its type parameter's bound, and not again when a concrete argument is copy.
- rejected: an agent proposed checking the spelling again at each concrete instance; the owner rejected it, because that would contradict writing a generic body once against its bound.

### decision copy-destructuring-without-move
Destructuring a copy struct uses the bare place and copies its fields, leaving the source live, while affine and linear values keep the `move` spelling.
- rejected: an agent proposed keeping destructuring to structs that are not copy; the owner rejected it, because under structural copy a struct of copy fields could then not be destructured at all.