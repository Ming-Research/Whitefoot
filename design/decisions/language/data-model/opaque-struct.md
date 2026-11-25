# Opaque structs

The `opaque` struct modifier, which gives a struct readable fields and no usable constructor, and the built-in types declared with it.

### decision opaque-structs-have-no-usable-constructor
A struct may carry `opaque`, written before `nocopy` or `nodrop`; it has ordinary fields, and a constructor call or a destructuring binding that names it is rejected.
- replaced: A table of built-in nominal types in the specification's prose, written as notation no source item could state, was on main; the owner changed it, because a type whose representation the host or the compiler owns should still be declared in the form a writer reads and writes.
- rejected: an agent proposed refusing the constructor call while admitting the destructuring binding; the owner rejected it, because `let TcpSend() = move s;` would then bind nothing and still discharge a linear host handle's must-consume obligation without closing it.
- rejected: an agent proposed the spellings `sealed`, `abstract` and `builtin` for the modifier; the owner rejected it, because the specification already calls these types opaque, so a second word would name a property the reader can already name.
- replaced: Giving both refusals one repair, to build the value with a construction function, was on main; the owner changed it, because a construction function forms only the prelude's storage shapes and cell, so that repair cannot be carried out for a host handle or a program's own opaque struct.

### decision built-in-types-are-declared-opaque-structs
The prelude declares `Box<T>` with its one field `inner` and the five storage shapes with readonly measures as opaque structs, and each std host module declares its host handles as opaque structs.
- rejected: an agent proposed leaving `Array`, `Slots` and `Ring` as types no source item declares; the owner rejected it, because an opaque struct's fields state the interface and not the layout, as `inner` names content in a separate heap object, and the two capacity forms are one type rule.
- replaced: The prelude declaring the fourteen host handles was on main; the owner changed it, because the host declarations had moved into the standard library, which places each host handle in the module interface that owns it.

### decision a-program-may-declare-an-opaque-struct
A program may declare an opaque struct, and since only a definition the build supplies forms an opaque struct's value, a program's own opaque struct never has one.
- rejected: an agent proposed admitting the modifier only on library declarations, judged by a declaration's origin; the owner rejected it, because a modifier that the library may write and a program may not would bring back the separate system declaration domain the prelude is deliberately not.

### decision opaque-is-a-struct-modifier-only
`opaque` is admitted on a struct alone, and there is no opaque enum.
- rejected: an agent proposed admitting `opaque` on enums too, for symmetry; the owner rejected it, because the types that need it, the cell and the host handles and values, are products of fields, and nothing else in the language asks for the modifier.

### decision host-opaque-structs-share-one-layout-ceiling
Every fieldless opaque struct outside the prelude has the layout ceiling (32,16), whatever representation the host links for it.
- rejected: an agent proposed layout ceilings chosen by the linked representation; the owner rejected it, because the sizes in a native catalog must not choose source behavior, or the ceilings that target qualification holds a layout to.
