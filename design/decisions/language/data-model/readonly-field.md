# Readonly fields

The `readonly` field modifier: who may write such a field, and on which fields it may be written.

### decision readonly-means-only-the-declaring-module-writes
Outside its declaring module a `readonly` field is never a write target, and inside it is an ordinary field; the prelude's readonly fields are never write targets, and a source one must be public.
- replaced: Listing the compiler-maintained members only in the specification's prose, with no declaration form a writer could use, was on main; the owner changed it, because those members should be ordinary fields of ordinary declarations, which a writer with the same need can also declare.
- replaced: A readonly field that no source code anywhere may assign was on main; the owner changed it, because a source type's own operations could then never update a published member in place, so only the prelude could use the modifier for its purpose.
- rejected: an agent proposed a different word, one that could not be read as a promise that the value never changes; the owner rejected it, because `readonly` is the word most readers already know, and a read-only member that its type's own operations change is its familiar meaning.
- rejected: an agent proposed admitting `readonly` on a private field; the owner rejected it, because a private field is already out of reach of other modules, so the modifier would restrict nothing.
