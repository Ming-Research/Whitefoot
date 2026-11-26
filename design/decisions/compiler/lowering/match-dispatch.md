# Match dispatch

How a loop around a `match` over a nominal enum is split into one function per arm chained by guaranteed tail calls, what the parts carry, and how a writer sees the result.

### decision a-match-loop-splits-into-tail-calling-arm-functions
A loop whose header ends in a `match` over a nominal enum becomes an enclosing function, an inlined dispatch and one function per arm, each back edge a guaranteed tail call through a handler table.
- replaced: One function with one `switch` for such a loop was on main; the owner changed it, because the E0 measurement put it 15 to 21 percent slower than arm functions chained by tail calls, in the forms Whitefoot can express (data: `research/experiments/match-dispatch/RESULTS.md`).
- rejected: an agent proposed computed goto inside one function; the owner rejected it, because it matched the arm functions only in an unchecked form with little live state, and fell back to the switch's cost with a checked interpreter's state.

### decision dispatch-parts-share-one-parameter-list
Every part takes one parameter list in one order: the result destination, register-carried header parameters, header values the arms read, values from before the loop it reads, and the frame pointer.
- rejected: an agent proposed a prototype for each part; the owner rejected it, because a guaranteed tail call requires identical prototypes.
- rejected: an agent proposed one shared dispatcher out of line; the owner rejected it, because inlining the dispatch function into each arm gives every arm its own indirect transfer.

### decision dispatch-parts-use-preserve-none-where-the-host-has-it
The parts use `preserve_none` where the clang's probe, or a package's record, shows it takes it with a guaranteed tail call, else the C convention; unchanging values past registers wait in the frame.
- rejected: an agent proposed requiring an LLVM that has the convention; the owner rejected it, because LLVM 18, still a supported host compiler, lacks it.
- rejected: an agent proposed passing the parameters that do not fit on the stack; the owner rejected it, because E0 measured a part passing one parameter on the stack 8% slower and two 43% slower, worse than the whole-function switch (data: `research/experiments/match-dispatch/RESULTS.md`).
- rejected: an agent proposed spilling values that change on every dispatch to the frame; the owner rejected it, because only a value that never changes costs just a load where it is read.
- rejected: an agent proposed forming the handler table's address in each arm; the owner rejected it, because it cost two instructions per dispatch, and the address is a parameter wherever a register is left for it.

### decision a-dispatch-slot-that-never-crosses-a-transfer-belongs-to-its-part
The enclosing function owns the frame and passes it to every part; a frame slot whose contents never cross a transfer between parts is each using part's own allocation.
- replaced: Keeping a slot in a part only when exactly one part used it was on main; the owner changed it, because a join copied into every arm counts each arm as a user, and Halo's `Call` and `Return` arms ran 14 dead stores per Lua call (data: `research/investigations/halo-call-path/DESIGN.md`).
- rejected: an agent proposed sharing every slot in the enclosing frame; the owner rejected it, because the host cannot remove a part's own temporary there, and the scrutinee's copy then made the WF interpreter's loop 12% slower than the whole function.
- rejected: an agent proposed passing each slot's pointer separately; the owner rejected it, because storage a value keeps across parts must outlive them in the enclosing activation, which one frame pointer gives.
- rejected: an agent proposed addressing slots from a pointer each part derives at entry; the owner rejected it, because it raised the wasm interpreter's CoreMark score only 0.9%, below the 2% criterion set for it (data: `research/experiments/match-dispatch/RESULTS.md`).

### decision some-dispatch-loops-are-emitted-whole
A function that waits, is a budget variant or sequential clone, has overlap groups, is compiler-made, or has a context or shared-record prelude is emitted whole; tables hold function addresses.
- rejected: an agent proposed storing a handler offset in place of the tag; the owner rejected it, because the E0 criterion for it was not met (data: `research/investigations/match-dispatch/DESIGN.md`).

### decision dispatch-loop-invariants-are-computed-once-before-the-loop
When the loop is entered by one edge, the enclosing function computes once what the loop cannot change, such as a read-only reference's box projections and measures, and passes it to the parts.
- replaced: Reloading loop-invariant values in every part was on main; the owner changed it, because the host cannot move those loads across the parts, and computing them once brought the WF interpreter to 1.00-1.13x of the C tail-call form (data: `research/experiments/match-dispatch/RESULTS.md`).
- rejected: an agent proposed passing every reference whatever the loop still reads of it; the owner rejected it, because a reference used only for its box's projection needs only that projection.

### decision a-box-stays-pinned-across-calls-that-cannot-replace-it
A passed-through reference whose box the loop keeps stays pinned when the loop hands it to non-waiting callees whose declared writes all lie below the box's content.
- rejected: an agent proposed refusing to pin a reference handed to any callee; the owner rejected it, because such a callee cannot replace the box, and keeping it raised the wasm interpreter's CoreMark score 3.8% (data: `research/experiments/match-dispatch/RESULTS.md`).
- rejected: an agent proposed reading the callees' bodies; the owner rejected it, because the lowering would then depend on implementations rather than checked boundaries.

### decision dispatch-parts-carry-the-element-address-beside-the-index
When the header's `match` reads the `Slots` element a header parameter indexes, the parts carry its address beside the index; an edge stepping by a constant moves it, others form it anew.
- replaced: Forming the element's address from the index in the header was on main; the owner changed it, because the moved address saves a move, a shift and two additions, and raised the wasm interpreter's CoreMark score 13.0% (data: `research/experiments/match-dispatch/RESULTS.md`).
- replaced: Moving the received address by the index's change on every edge was on main; the owner changed it, because on Halo's Lua interpreter it made the `loop` kernel 5.7% slower, with a subtraction more on each such edge (data: `research/experiments/match-dispatch/RESULTS.md`).
- rejected: an agent proposed carrying the element's address in place of the index; the owner rejected it, because it scored 1.6% below the address beside the index, below it in every pair (data: `research/experiments/match-dispatch/RESULTS.md`).

### decision a-mixed-join-carries-the-element-address-through-a-pointer-join
At a join merging a stepped index with another that only tests it below a bound, the test moves onto each incoming path, and the join gets the element's address through a pointer join.
- replaced: Forming the address from the run on every path into such a join was on main; the owner changed it, because the pointer join raised the wasm interpreter's CoreMark 1.3%, Halo unchanged, kept as an extension of the step analysis (data: `research/investigations/match-dispatch/DESIGN.md`).
- rejected: an agent proposed forming the moved address before the bounds test; the owner rejected it, because it would form an address before the subscript's bounds judgment [OP-4] admits it, which the backend may treat as poison.

### decision the-dispatch-ledger-reports-each-split-verdict
`whitefootc --dispatch-ledger` prints, for each loop around a `match` over a nominal enum, whether it was split or the first condition it failed, read from comments the emitter puts in the module.
- rejected: an agent proposed a source marker that requires the split; the owner rejected it, because whether a loop splits depends on the target's argument registers and the toolchain, so one program would be accepted on one host and refused on another.
- rejected: an agent proposed a notice printed on every build; the owner rejected it, because Whitefoot's diagnostics are kept for rejections and have no place for it.

### decision whole-program-dispatch-families-get-handler-words
In a whole-program build, each value of a split family's enum carries a hidden word per family holding its arm's address, when the families fit the layout ceiling and only emitted code builds values.
- replaced: Dispatch through a tag-indexed table on those edges was on main; the owner changed it, because the owner chose an address in each value (data: `research/investigations/match-dispatch/DESIGN.md`).
- rejected: an agent proposed handler addresses visible in source; the owner rejected it, because the owner chose a compiler-private word in each value, not a source form.

### decision integer-match-loops-are-not-split
A loop around an integer `match` keeps ordinary switch lowering; only nominal-enum loops are split.
- rejected: an agent proposed dense synthetic tags for integer values, or a table spanning the integer domain; the owner rejected it, because handler tables and words are indexed by a closed nominal layout, and sparse integers give neither that layout nor its address contract.