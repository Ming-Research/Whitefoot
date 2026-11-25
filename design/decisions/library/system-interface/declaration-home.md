# Declaration home

Where the types and signatures the host supplies are declared, and in what source form.

### decision host-declarations-live-in-std-interfaces
Host types and signatures are ordinary public declarations of standard library module interfaces, under every module rule, with definitions the build supplies from its runtime units.
- replaced: A distinct compiler-owned declaration domain for system types and operations was on main; the owner changed it, because it told otherwise ordinary values and functions apart by implementation origin alone.
- replaced: Host types and signatures declared in the prelude was on main; the owner changed it, because a prelude declaration is injected into every module whether or not it names it, which costs every check (data: `research/investigations/library-modules/DESIGN.md`).
- rejected: an agent proposed host declarations visible only to a program of a declared kind; the owner rejected it, because the condition would partition no accepted program and only keep a dead declaration path.
- rejected: an agent proposed a callable a function formal accepts only with a Whitefoot body, not a host signature; the owner rejected it, because native binding is a link choice and gives no reason to exclude the same ordinary signature.
- rejected: an agent proposed a source modifier such as extern, native or host on declarations the build supplies; the owner rejected it, because it tells declarations apart by implementation origin, and which ones the build supplies is build metadata keyed by their identities.

### decision host-handles-are-fieldless-opaque-structs
Each fieldless host handle type is a public opaque struct with no fields in its standard library module's interface, `nodrop` where it must be closed and `nocopy` otherwise.
- replaced: A table of the host handle types as opaque nominal types, in notation no source syntax expresses, was on main; the owner changed it, because the writer had types whose declaration form source could not use, while the opaque modifier states the same in the writer's form.
- replaced: Host handle types written in the prelude was on main; the owner changed it, because a handle belongs in the standard library module that owns it, not in a prelude visible everywhere.