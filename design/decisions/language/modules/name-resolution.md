# Name resolution

How a module's records declare, publish and name declarations: interfaces, visibility, aliases, qualified paths and the identifier domains.

### decision one-module-wfm-holds-the-whole-interface
A module's one `module.wfm` holds its complete public signatures, types, contracts and a doc entry per function, with no function bodies, and only it publishes.
- rejected: an agent proposed a thin export list that selects public definitions whose full signatures live only in `.wf` files; the owner rejected it, because a caller and an implementer need one complete handwritten contract in the `.wfm` itself.
- rejected: an agent proposed a publication switch in implementation files beside the `.wfm` declaration; the owner rejected it, because the interface declaration already owns publication, and a second switch hides that authority.
- rejected: an agent proposed an interface split into fragments, or declarations without a doc entry; the owner rejected it, because a caller and an implementer need one complete contract with its description.
- rejected: an agent proposed executable bodies in the public interface; the owner rejected it, because reviewing the public contract must stay separate from editing bodies.
- rejected: an agent proposed declaring a module's name apart from its directory; the owner rejected it, because the directory layout must not disagree with a separately declared module name.

### decision declarations-and-fields-default-to-private
Top-level declarations and struct fields are private to their module unless marked `public`, which only `module.wfm` may write, and only where it creates an access route [MOD-6].
- rejected: an agent proposed public by default with private exceptions; the owner rejected it, because adding representation detail must not silently widen the callable or field API, and an explicit publication surface keeps change review focused.
- rejected: an agent proposed different defaults for top-level declarations and struct fields; the owner rejected it, because adding representation detail must not silently widen the field API either.
- rejected: an agent proposed modifiers that publish nothing, such as `private`, or `public` in a `.wf` file or on a member of a private type; the owner rejected it, because an explicit publication surface keeps change review focused.

### decision one-accessibility-rule-for-code-and-annotations
Every name and field that a body, contract, invariant, proof premise, effect row or function-kind formal writes must be accessible where it is written [MOD-5].
- rejected: an agent proposed letting contracts, invariants, premises and effect rows name private fields that code cannot; the owner rejected it, because clients would depend on fields not marked as API and lookup would need a rule per role.
- rejected: an agent proposed named specification projections and effect regions in place of field paths; the owner rejected it, because they only keep client source unchanged when a published representation changes, which the module goals do not require.
- rejected: an agent proposed getter calls in contracts; the owner rejected it, because the interface must state what its contracts depend on, so that changing such a field is an explicit interface change.

### decision pkg-and-std-are-fixed-qualifiers
`pkg` names the source's own package and `std` the standard library shipped with the toolchain, which a graph row lists and a source bundle may name without a row.
- replaced: `pkg` as the owning selected root, with all library composition deferred, was on main; the owner changed it, because host declarations then cost every check 36 to 62 percent of its instructions (data: `research/investigations/library-modules/DESIGN.md`).
- rejected: an agent proposed user-selected prefixes such as app, lib or project; the owner rejected it, because one package-oriented spelling serves applications and libraries alike.
- rejected: an agent proposed reading every imported source against the primary root; the owner rejected it, because a library's own paths must keep their meaning whatever name an importer gives the library.
- rejected: an agent proposed every standard library declaration visible unnamed in every module; the owner rejected it, because it bypasses the graph's one dependency authority and the refusal of glob imports, and makes every check pay for names it never uses.
- rejected: an agent proposed binding the standard library through a graph's package binding; the owner rejected it, because a library shipped with the compiler needs no location, identity or version, and a binding would only let a program name another library.

### decision module-records-are-module-wfm-and-direct-wf-files
A registered module's records are the `module.wfm` in its directory and the `.wf` files directly beside it [MOD-2].
- rejected: an agent proposed sibling `name.wfm` files, or several supported layouts; the owner rejected it, because a module's contract and bodies should share one work location.
- rejected: an agent proposed `directory/name.wfm` interface files; the owner rejected it, because the file name would repeat its directory's name.
- rejected: an agent proposed assigning any file to any module; the owner rejected it, because that needs another map of member files to maintain.

### decision a-module-is-one-inventory-across-its-files
A module's direct `.wf` files share one declaration inventory, a child directory with its own `module.wfm` is another module, and a directory without one is only a naming location.
- rejected: an agent proposed one module per file; the owner rejected it, because files organize one private implementation, while only a separate complete interface makes another module boundary.
- rejected: an agent proposed routing calls between a module's own files through its `.wfm`; the owner rejected it, because the files are one private implementation.
- rejected: an agent proposed namespace blocks, or ancestor directories owning their children implicitly; the owner rejected it, because only a separate complete interface makes a module boundary.

### decision another-module-is-named-through-its-graph-row
A name of another module resolves to its canonical identity, needs that module in the naming module's graph row, and reaches only its public declarations and fields.
- rejected: an agent proposed dependency lists in source files; the owner rejected it, because the graph is the one dependency authority.
- rejected: an agent proposed implicit access to earlier modules; the owner rejected it, because permission comes only from the one dependency authority, the module's graph row.
- rejected: an agent proposed name aliases defined in the graph; the owner rejected it, because explicit name bindings in the records keep permission apart from what a name means.
- rejected: an agent proposed mandatory forwarding through intermediate modules; the owner rejected it, because visibility is not transitive.
- rejected: an agent proposed dependency rules based on parent and child directories; the owner rejected it, because related modules get no family privileges.

### decision aliases-are-file-local-headers
An alias header at the start of a record binds short names to complete accessible paths for that record alone, with no chaining, publication or graph edge [MOD-4].
- rejected: an agent proposed full paths at every use; the owner rejected it, because long names need local abbreviations.
- rejected: an agent proposed module-wide alias headers, or implementation files inheriting the interface's aliases; the owner rejected it, because a private abbreviation edit would then affect several files, and a file's names would need another file's context.
- rejected: an agent proposed public re-exports through aliases; the owner rejected it, because shared alias state would couple independent files and make interfaces depend on implementation context.

### decision place-bases-stay-unqualified
A qualified path names a type, group, callee or constructor, while a place base, a `const` term and a `cvalue` stay single identifiers, so another module's constant is read through an alias.
- rejected: an agent proposed qualified place bases and constant terms through a shared operand prefix; the owner rejected it, because the place base would leave `place`, so MSR-1, ENT-2, SET-1 and EFF-1 would each need a second place shape for what an alias already spells.

### decision qualified-constant-reports-the-alias-repair
A qualified path in a value position that continues no call is rejected over the path, citing MOD-5, or GRAM-9 in an atom position, with a repair naming the alias line to add.
- rejected: an agent proposed letting a qualified path name another module's constant directly; the owner rejected it, because the cost the existing refusal names is unchanged, and the writer's trouble, a report on the wrong line, is fixed in the diagnostic.

### decision module-declarations-are-visible-throughout-the-module
A module's top-level declarations are visible in its records whatever the file and item order, aliases only in their record, and local binders lexically; visibility grants no value or proof.
- replaced: top-level functions visible throughout one closed unit, other declarations only after their own, was on main; the owner changed it, because per-module inventories make a top-level name's meaning independent of file layout and keep unrelated modules' names apart.
- replaced: a top-level function visible only after its declaration was on main; the owner changed it, because item order then changed meaning, and direct mutual recursion could not be written without reordering or indirection.
- rejected: an agent proposed treating a declaration's visibility as its validity; the owner rejected it, because being in the inventory grants no value or proof, so constant dependencies, instantiation and layout are checked apart.

### decision implementations-repeat-the-interface-header
A function's definition repeats its interface declaration's signature, bounds, effects and contracts exactly, without `public`, and the interface's types and constants are used as written there.
- rejected: an agent proposed definitions that refine their declarations under FN-4; the owner rejected it, because callers read only the declaration and gain nothing from a weaker requirement, a stronger guarantee or a narrower row.
- rejected: an agent proposed unchecked header drift, or a body-only form that binds to the declaration; the owner rejected it, because the implementation must acknowledge an interface edit in the same written form, and repeating a checked header costs little.
- rejected: an agent proposed treating interface declarations as trusted theorem imports; the owner rejected it, because declaration correspondence, the body's proof and current recursive summaries are still needed.

### decision public-structs-are-defined-whole-in-the-interface
A public struct has its one complete definition in `module.wfm`, public and private fields together, while implementation-only types may stay in `.wf` files.
- rejected: an agent proposed publishing only whole records; the owner rejected it, because one field list must support both direct data access and private state.
- rejected: an agent proposed a name-only public declaration with its fields in `.wf`, or a public projection matched to a second full definition; the owner rejected it, because one field list and declaration order avoid duplicated representations.
- rejected: an agent proposed private field extensions; the owner rejected it, because one field list avoids merged representations.

### decision field-operations-need-access-to-named-fields
A field operation needs access to each field it names; outside its module a construction needs every field public and not readonly, and a destructuring covers private fields with `..`.
- rejected: an agent proposed banning all extraction outside the module from types with mixed fields; the owner rejected it, because the complete interface representation makes the leftover obligations checkable without granting private access.
- rejected: an agent proposed permitting holes in a destructuring; the owner rejected it, because a linear resource could then be left unconsumed.

### decision enum-variants-take-their-enums-visibility
A public enum's variants are all public and a private enum's private, with no marker per variant, and public groups and bindings publish their complete shapes.
- rejected: an agent proposed a public marker on each variant; the owner rejected it, because it can only repeat the enum's own visibility.
- rejected: an agent proposed hidden enum cases, which need a new non-exhaustive matching mechanism; the owner rejected it, because a closed-case API needs its full shape, while private representation can stay behind a public struct.
- rejected: an agent proposed hiding some formal parameters of a public group; the owner rejected it, because a callable-bundle API needs its full shape.

### decision variants-belong-to-their-enum
A variant belongs to its enum: a construction names the enum before it, and a match arm or route label resolves against the enum it already has [TYPE-6].
- rejected: an agent proposed one global inventory of unique constructor names; the owner rejected it, because distinct modules and enums must not compete in one global variant inventory.

### decision source-paths-do-not-depend-on-the-host
Module paths are lowercase components, and a symbolic link, or two names in one directory that differ only in letter case, stops the build as an input failure [MOD-2].
- rejected: an agent proposed following several physical aliases into one source module; the owner rejected it, because module membership must not depend on how the host resolves paths.
- rejected: an agent proposed treating filesystem uniqueness as complete name validation; the owner rejected it, because declarations across files and child modules can still collide, and path ambiguity must be reported the same way, not settled by host lookup order.

### decision invariant-names-take-any-identifier
An invariant name may be any lexical IDENT, operation names and mode words included, in its own proof-only domain [TYPE-6].
- replaced: reserving operation and mode words from invariant names, as DIAG-1 did, was on main; the owner changed it, because invariant names are neither callable nor field names, so neither operation lookup nor OPNAME maximal munch gives a reason to exclude them.

### decision counted-loop-binders-reserve-like-let
A counted-loop value binder may not be an operation name or mode word, as a `let` binder may not [OP-1].
- rejected: an agent proposed allowing those words in counted-loop binders while `let` reserves them; the owner rejected it, because the binding syntax sets up no separate lookup domain, unlike proof-only invariant names.

### decision callables-and-values-compete-apart
Lexical identifiers split into callables and values, a module alias being both, and outside one module's inventory two declarations of a spelling compete only within a class.
- replaced: one undivided identifier domain, where a local value beside a function of its spelling was a shadow, was on main; the owner changed it, because that refusal protects no reading and cost every merge of separate records renames (data: `research/investigations/layout-friction/DESIGN.md`).
- rejected: an agent proposed locals that may shadow every module-level declaration; the owner rejected it, because a local could then hide a named const in a value position, so reading a name would need the local scope.
- rejected: an agent proposed declarations private to the record that declares them; the owner rejected it, because it adds a modifier, and moving a declaration between records would change what it sees, to save one more rename in the measured merge.

### decision one-inventory-competes-across-classes
Two declarations in one module's inventory compete in their domain whatever their class [TYPE-6].
- rejected: an agent proposed an alias that binds every declaration its path names, one per class; the owner rejected it, because the alias would then stand for two declarations, and reading it would need the use beside it.