# Language

Rules for everything the language defines, now and later: how a program is accepted, how it is written, and how its parts see each other.

### principle acceptance-by-fixed-rules
Whether a program is accepted follows from fixed rules alone: no search, timeout, machine speed, solver state or work budget decides the verdict.
Why: A writer, human or agent, can then tell from the rules whether a program will be accepted, and one program gets one verdict on every machine and every run.

### principle one-meaning-one-spelling
Each meaning has one spelling in the language. A short form kept because it is easier to write expands mechanically into that one spelling.
Why: A writer then never chooses between equal forms and a reader never compares them; a short form that expands mechanically, as a text literal does into its bytes, keeps both easy.

### principle the-signature-is-the-boundary
A caller, and every other module, reads only a function's interface: its signature, effect row and contracts, never its body.
Why: A body can then change without changing any verdict outside the body, so each module is checked on its own and a change stays local.

### principle one-path-overlap-judgment
Ownership, effects and parallel permission decide whether two accesses overlap by one and the same judgment of paths.
Why: The three checks then cannot disagree about the same two accesses, and a new kind of place or access changes one rule, not three.
