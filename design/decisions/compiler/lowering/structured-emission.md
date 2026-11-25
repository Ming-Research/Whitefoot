# Structured emission

How the backend builds an LLVM module as a structured model before printing it as text, and what that model holds.

### decision llvm-emission-builds-a-structured-module-before-text
The emitter builds a model of the module's functions, blocks, declarations, types, attribute groups and symbols, with each block's real exit label, and prints LLVM text from it.
- replaced: An emitter that predicted split blocks apart from building them, inserted text at byte offsets and parsed its own LLVM again was on main; the owner changed it, because it had to recover ownership and dependencies from its own finished output.
- rejected: an agent proposed adopting an LLVM library in this change; the owner rejected it, because what was missing is a contract between this compiler's emission and its fragment building, and an outside IR adds work without being needed for that.
- rejected: an agent proposed a second optimization IR in the emitter; the owner rejected it, because lowering already owns the typed control-flow graph, and the model needs only the structure and dependencies for correct printing and splitting.

### decision integer-match-lowers-to-one-llvm-switch
An integer match has its own IR switch terminator with constant targets and an optional default, and is emitted as one LLVM switch over the scrutinee evaluated once.
- rejected: an agent proposed lowering it to a chain of equality tests; the owner rejected it, because one switch over the scrutinee, evaluated once, covers its sparse constants, and complete coverage needs only an unreachable default.
- rejected: an agent proposed reading the integer as an enum discriminant; the owner rejected it, because its constants are sparse values of the source integer's width, not enum ordinals.

### decision identical-external-declarations-are-emitted-once
Identical external declarations share one declaration in the module model, and declarations that disagree stop emission.
- rejected: an agent proposed overwriting a conflicting signature, or filtering the printed LLVM text; the owner rejected it, because separate function builders reach the same runtime ABI, and every fragment must keep its one authoritative declaration.