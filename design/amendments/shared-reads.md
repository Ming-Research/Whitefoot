Node: language/waiting/shared-objects/shared-maps

Replaces the decision beginning "Provisionally, every keyed statement holds its entry exclusively whether its block reads or writes", whose reopening condition, a workload whose readers of one key contend such as the benchmark's `LRANGE` tests, the many-core run met.

Decision: A keyed statement whose guard and block write nothing through its binding holds its entry beside the other such statements on its key, and nothing in the source marks it, because those statements' reads take effect at one point each in one order whichever of them runs first [SHARE-3], so sharing changes no outcome, while every client of the benchmark's four `LRANGE` tests reads one list, which firn answered at 0.44 to 0.61 of Garnet on 4 to 16 server CPUs with exclusive holds ([many cores](../../research/investigations/concurrent-map/DESIGN.md#many-cores), [shared reads](../../research/investigations/concurrent-map/DESIGN.md#shared-reads-of-one-key)), instead of a read form the writer marks, which asks the writer to choose for a difference no program observes, or reads that check a version afterwards, which a block that runs once cannot redo after a torn read.

Rejected:
- Replaced decision beginning "Provisionally, every keyed statement holds its entry exclusively whether its block reads or writes": rejected because its reopening condition was met, readers of one list serializing firn's `LRANGE_100` at 1.24 to 1.55 million a second on 2 to 16 server CPUs while Garnet grew to 2.83 million.
- A read form the writer marks: rejected because the outcome is the same either way [SHARE-3] and the checker already knows which paths the block writes.
