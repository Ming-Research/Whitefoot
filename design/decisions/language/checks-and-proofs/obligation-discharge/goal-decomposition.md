# Goal decomposition

How a Boolean goal is split into its parts and rebuilt from them.

### decision boolean-goal-decomposition
A Boolean goal is split at its establishment site through its conjunction, disjunction and negation structure, and the same fixed rules rebuild parent goals.
- replaced: atomic goal evidence with no decomposition was on main; the owner changed it, because a negated disjunction guard and a conjoined range check were rejected although complete evidence was in hand.

### decision no-formula-synthesis
Composition of arbitrary formulas stays excluded.
- rejected: an agent proposed formula synthesis; the owner rejected it, because the fact state holds only facts true on every path, and a true disjunction fixes neither member.