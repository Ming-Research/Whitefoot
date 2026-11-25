# Reference validity

What a reference names, how its path is spelled and evaluated, and the events that end it.

### decision reference-names-a-path
A reference names a path from a local, a parameter or a named constant through fields, `^`, indices, ranges or payloads; it is not storage, so a reference to a reference variable is refused.
- rejected: an agent proposed a pointer value with an identity of its own; the owner rejected it, because a name for a path holds no address a program could copy or alias, and it can always be formed again.

### decision postfix-caret-step
Ordinary and proof places reach a referent with one postfix `^` step, as in `p^`, `p^.field` and `p^[i]`.
- replaced: The prefix form `deref(p)` was on main; the owner changed it, because postfix `^` composes paths in reading order with one punctuation byte and no clash with arithmetic tokens.
- rejected: an agent proposed separate forms for the whole referent and for a field, as in C's `*p` and `p->f`; the owner rejected it, because one postfix `^` composes paths in reading order with one punctuation byte.
- rejected: an agent proposed reaching a referent's fields with no written step; the owner rejected it, because one explicit postfix `^` composes paths in reading order with one punctuation byte.

### decision payload-reference-outlives-match
A checked reference to an existing payload keeps its selected place after the selecting match ends, while a newly written payload selection still needs a current variant fact.
- replaced: Ending a payload reference when its selecting arm ends was on main; the owner changed it, because ending a lexical proof scope does not change the selected storage, and iterative descent must carry its selected address across the backedge.

### decision index-evaluated-at-formation
An index inside a reference's path is evaluated once when the reference is formed, and the path keeps that value.
- rejected: an agent proposed reading the index bindings again at every use; the owner rejected it, because a later assignment to the variable that gave the index must not silently retarget a live reference.

### decision events-that-end-validity
A reference becomes invalid when its target or an ancestor is moved or released, a proper ancestor is structurally written, its root leaves scope, or a window operation destroys its selection.
- rejected: an agent proposed keeping a reference usable across a write to a proper prefix of its path when its storage is provably untouched; the owner rejected it, because that write may replace, reallocate or release what the rest of the path was read against.
- rejected: an agent proposed ending a reference when the storage at its path is written; the owner rejected it, because changing the value a reference names is the ordinary use of a reference.
- rejected: an agent proposed letting the preservation of a pointer across a leaf store also keep facts about the stored value; the owner rejected it, because validity says one captured place still exists, while facts describe its current contents.

### decision call-protects-only-own-target
A reference passed to a call is protected only from content writes at or below its own target, and another argument's destructive write to an ancestor still invalidates it.
- rejected: an agent proposed exempting every reference passed to a call from invalidation after the call; the owner rejected it, because one argument can replace an ancestor while another, unused argument names its destroyed descendant.

### decision move-never-reroots-references
A move never re-roots a reference: references formed through the old place become invalid rather than pointing into the new owner.
- rejected: an agent proposed retargeting those references at the move; the owner rejected it, because the path they name has ceased to exist even when the bytes did not move.

### decision assignment-evaluates-right-side-first
`set p = e;` evaluates `e` first and then the target in the state `e` left, so an assignment means binding `e` and assigning the binding.
- rejected: an agent proposed evaluating the target first and holding its storage valid across the right-hand side; the owner rejected it, because it adds a concept and a refusal, refuses clear programs that regrow or replace the target's storage, and differs from binding first.
- rejected: an agent proposed forbidding every right-hand side to write, move out of or release a prefix of its target; the owner rejected it, because an ordinary assignment leaves no vacant place during its right-hand side, so the rule refuses safe programs, and it keeps the target evaluated first.
- rejected: an agent proposed reading the target's subscripts before the right-hand side and locating the place after it; the owner rejected it, because it joins two moments of evaluation and still differs from binding first when the right-hand side changes a subscript's source.