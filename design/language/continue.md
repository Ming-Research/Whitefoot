Decision: `continue;` transfers to the nearest enclosing ordinary or counted loop and `continue @label;` to the lexically enclosing loop with that label, carrying current bindings through the ordinary cleanup and next-header proof obligations, with a counted target performing its increment exactly once, because early next-instruction paths inside nested interpreter work need the same checked backedge as fallthrough instead of extra conditional nesting.

Rejected:
- A value-carrying transfer: rejected because assignment already supplies the backedge values, whereas transfer arguments would require a second state-binding interface, evaluation order and affine-move rules with no demonstrated additional consumer.
- Unlabeled continue only: rejected because an early next instruction inside a nested decoding or copy loop must reach the outer interpreter loop, and existing lexical labels supply that target without scope entry.
- Keeping only fallthrough and break: rejected because it excludes natural early iteration transfer without establishing a safety or proof advantage.
