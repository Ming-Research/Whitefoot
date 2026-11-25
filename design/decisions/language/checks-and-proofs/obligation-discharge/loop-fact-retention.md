# Loop fact retention

Which facts survive a loop head and a loop exit.

### decision pre-loop-facts-survive-exits
A pre-loop fact survives a loop head unless a body kill can reach a later head without leaving the body, since return, error and break paths cannot reach that head.
- replaced: killing head facts on every edge leaving the scope was on main; the owner changed it, because one return inside a loop body dropped every pre-loop fact, so a program was accepted with the return outside the loop and rejected with it inside.

### decision header-conclusions-leave-loop
A proved loop-header invariant conclusion leaves its loop on every exit edge and survives the continuation join, while the invariant's name leaves scope with the body.
- replaced: removing header conclusions on every edge leaving their loop was on main; the owner changed it, because writers restated `pos <= length` after each `break`, 11 of the URL parser's 23 clamp blocks (data: `research/investigations/writer-lost-facts/DESIGN.md`).
- rejected: an agent proposed proving the header again at every `break` as an obligation; the owner rejected it, because a `break` taken after the loop's variables changed would turn accepted programs into rejections.
- rejected: an agent proposed proving the header again at every `break` and publishing only what succeeds; the owner rejected it, because every measured `break` leaves its variables unchanged since the head, and a judgment that fails silently would be new to invariant checking.

### decision head-kill-without-facts
Whether a continuing kill may kill a pre-loop fact at the loop head is judged without facts: two index, range or page steps at one position always overlap there.
- rejected: an agent proposed separating offsets with the pre-loop facts that no continuing event writes; the owner rejected it, because it adds a second implicit pass to the head state, while a header invariant already states and proves the fact the writer needs.
- rejected: an agent proposed separating offsets only when both are literals or named consts; the owner rejected it, because writers would see separation by `1_u64` but not by a binding the body never writes.