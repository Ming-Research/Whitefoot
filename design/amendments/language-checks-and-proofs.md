Node: language/checks-and-proofs

Decision: A type invariant is written `invariant name(binder): left op right;` after its struct's fields, naming the value it constrains with an explicit binder that only its relation sees, and the relation is judged as the requirement clause of a function whose one parameter is that binder, because Whitefoot has no receiver and every other clause names its values by declared names, so the binder is an ordinary declared name and the relation reuses the requirement clause's checked forms, diagnostics and repairs unchanged, while the name, as a local invariant's name does, identifies the invariant in each obligation and diagnostic ([TYPE-11], [investigation](../../research/investigations/io-model/CONCURRENCY-MODEL.md#56-an-invariant-for-the-whole-life-of-a-value)), instead of an implicit `self` or bare field names.

Rejected:
- An implicit `self` binder: rejected because no other construct has a receiver, so `self` would be a reserved word that names a value in this one clause.
- Bare field names in the relation: rejected because a field name alone is a place in no other clause, so judging the relation would need a second name-resolution rule beside the requirement clause's.
