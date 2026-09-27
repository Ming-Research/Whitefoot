Node: language/system-interface/handle-factory

Decision: Acquisition and close take the handle factory as a reference parameter that their declared effect row names as written, spending one credit on successful acquisition, restoring it on refusal and returning it on the explicit close that releases the allocation, because the call's own comparison of its substituted effect paths already refuses any second access to that same factory within the call, so the accounting stays visible without any exclusivity marker on the reference, implicit creator links, or cancellation effects, instead of hidden global capacity or one-shot permits with implicit cancellation.

Decision: `factory_share` returns a second factory drawing on the same budget, so that a context started with value arguments carries its own factory while the host limit stays one budget, because a started call cannot borrow its starter's factory [PAR-4], instead of splitting the credits into fixed partitions that refuse an acquisition while another partition holds unused credits ([design](../../research/investigations/io-model/WAITS.md#a-shared-handle-budget)).

Rejected:
- Exclusive factory loans with an early-release prohibition: rejected because the factory's declared write and ordinary pairwise call check already exclude competing access without a second endpoint rule.
- A reservation whose drop silently returns factory credit: rejected because the selected ordinary interface has empty opaque drop and explicit host-state writes; a later reservation API would need explicit consuming return and cancellation outcomes.
- Available factory capacity as a promise that host acquisition succeeds: rejected because other host activity and limits can still refuse an otherwise funded attempt, so refusal remains an ordinary outcome.
