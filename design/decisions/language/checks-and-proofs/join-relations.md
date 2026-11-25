# Join relations

How a loop header's written relations survive the joins inside its body and reach the next header.

### decision joins-carry-header-relations
A join carries an enclosing loop header's written relation to the joined values when every input that is not contradictory proves it, and the next header is proved on each incoming edge.
- rejected: an agent proposed proving the header only at the back edge, widening each scalar on its own, or carrying inferred relations; the owner rejected it, because a guarded update, a conditional length-preserving call and correlated updates each lost a written relation at the join.

### decision merges-form-one-frontier
Consecutive merges with no real event between them form one frontier, joined once at the next real statement, with releases and scope kills applied to each input in source order.
- rejected: an agent proposed publishing at each merge, or saturating the set of written relations; the owner rejected it, because an inner result could then prove more at an outer join, so nesting the same branches would change what is accepted.

### decision next-slot-target-instance
At a counted loop's next header, a measure whose index holds the counted binder is a separate target-instance term, identified by the place's proof path as ordinary terms are.
- rejected: an agent proposed identifying the target instance by its source spelling; the owner rejected it, because one place would then have two identities, and a loop invariant written through an exact alias would be refused for no reason.