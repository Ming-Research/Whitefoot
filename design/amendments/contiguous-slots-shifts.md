Node: compiler/prelude-records

Decision: Retain the existing common logical-index walk for Slots insertion/removal instead of adopting the tested bulk-transfer replacement, because the [completed comparison](../../research/experiments/container-representation/vector-library/RESULTS.md#slots-final-code-and-paired-timing-regression-prevents-selection) removed native shift loops but regressed small scalar reuse by 13.9-14.5%, failing its recorded selection criterion. This is a proposed disposition of the earlier amendment, not an owner ruling; production code has been restored and the candidate with its passing regression tests remains a research patch. Ring, append, splitting and proved-empty-release commitments stay unchanged. Reconsider bulk transfer only with evidence that addresses the measured regression.

Rejected:
- Adopting the proposed Slots insertion/removal replacement: rejected because the contiguous alternative regresses small scalar reuse despite eliminating native shift loops. This is the proposed disposition, not an owner ruling. The live common-walk decision remains in force, including its Ring, append, splitting and proved-empty-release commitments.
