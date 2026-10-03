Node: compiler/storage-placement

Decision: Recommend refusing the proposed fixed-storage OP-11 exchange operation with one target-planned temporary of at most 16 bytes, because the [complete PriorityQueue pair](../../research/experiments/container-representation/priority-library/RESULTS.md#fixed-storage-exchange-timing-three-gains-and-six-useful-losses) passes correctness but records six qualified useful losses alongside three gains and fifteen overlaps, instead of adopting a smaller scratch representation that fails the experiment's no-regression criterion. Keep the existing lowering and generic representation/ownership behavior tests; the linked record preserves the proposed mechanism, qualifications, withdrawal and recommended refusal pending the owner's ruling.

Rejected:
- Reinterpreting chunks as arbitrary integer or vector values: rejected because padding and inactive bytes need not be initialized values, and pointer representations require ordinary memory-transfer semantics.
- Granting swap parameters noalias or forcing code shape with volatile copies, barriers, new inline hints or runtime alias branches: rejected because OP-11 admits equal targets and this comparison tests ordinary optimization of its existing checked domain.
