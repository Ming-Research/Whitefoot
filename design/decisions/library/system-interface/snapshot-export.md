# Snapshot export

How source requests a restricted snapshot of a map, what the export holds and encodes, how its records stream, and what its fork route promises.

### decision snapshot-capture-is-a-typed-host-function
Source requests a restricted snapshot through the generic host function `std::snapshot::capture_export`, given a prepared linear reserve, a whole map and a sequence cut.
- rejected: an agent proposed a restricted source callback run in the child; the owner rejected it, because its call and release closure would need a new child-safe callable contract, while the fixed encoder runs no user code and meets the consumer's need.
- rejected: an agent proposed a dedicated capture statement; the owner rejected it, because the operation fits an ordinary call inside the existing atomic statement, and a second statement form would duplicate its holding and ownership boundary.
- rejected: an agent proposed a privileged direct capture call; the owner rejected it, because wrappers and function formals cannot carry its held-origin relation, while the general held-observation contract does.

### decision snapshot-cut-is-held-by-reference
The exporter takes its cut by reference and declares the general held-observation contract over its map and cut.
- rejected: an agent proposed tracing the cut through the history of scalar values; the owner rejected it, because the same ordinary contract protects and forwards source helpers and linked functions with no privilege of origin.

### decision snapshot-image-at-one-effect-point
The map and cut stay unchanged through the caller's atomic statement, and the exporter keeps their image at the statement's single effect point.
- rejected: an agent proposed another visible capture point inside the statement; the owner rejected it, because a map in between, followed by more writes, need not be a dataset in the atomic history.

### decision snapshot-exportability-follows-ownership
A type is exportable when every owned field, payload, storage element and Box content is, recursive owned trees included, while shared, frozen and host handles are not.
- rejected: an agent proposed accepting only shallow fields; the owner rejected it, because source ownership then names the whole private-memory image the encoder may traverse.
- rejected: an agent proposed refusing an unexportable value at run time; the owner rejected it, because the refusal would come after a resource had entered the child.

### decision snapshot-format-encodes-logical-values
The fixed format encodes typed logical values with the value type as its schema, and the standard HashMap and OrderedMap as logical pairs.
- rejected: an agent proposed copying native layouts or the collections' backing representation; the owner rejected it, because buckets, deleted slots, tree nodes, addresses and padding are not a consumer's dataset.

### decision snapshot-floats-keep-every-bit
Floating-point export keeps every stored IEEE bit, signed zero, infinities and NaN payloads included.
- rejected: an agent proposed decimal formatting or canonical NaNs; the owner rejected it, because an export observes the stored value, which is not an arithmetic conversion.

### decision snapshot-records-stream-with-markers
Records stream through bounded segments with separate record and stream completion markers, and a successful export also needs a confirmed successful child outcome.
- rejected: an agent proposed buffering a whole record; the owner rejected it, because explicit completion markers already tell a pipe that ended after a partial record from a complete dataset.
- rejected: an agent proposed reading end of file as success; the owner rejected it, because a pipe that ends after a partial record is told from a complete dataset only by explicit completion evidence.

### decision snapshot-job-owns-collection-and-reap
A linear job owns the collection and exactly one confirmed reap, with separate observations of the child, the collection and an abort request.
- rejected: an agent proposed one success flag for the job; the owner rejected it, because a child's success can coexist with discarded bytes or a failed collection, and an abort can race with normal completion.

### decision snapshot-fork-route-is-research-only
The fork route behind `capture_export` is research-only, promising bounded encoder workspace and an output quota that fails on exhaustion, while no source operation forks.
- rejected: an agent proposed restricted fork as the source abstraction; the owner rejected it, because it needs a complete child execution contract and another route on Windows and targets without an MMU.
- rejected: an agent proposed a general fork returning into both source continuations; the owner rejected it, because copied parent contexts, host handles and runtime queues have no valid child continuation contract.
- rejected: an agent proposed presenting sampled memory and poll-and-kill as a hard bound; the owner rejected it, because the available reserve cannot enforce the owner's service-first footprint and cleanup promise.
- rejected: an agent proposed a silent scan, an eager copy or another platform's substitute; the owner rejected it, because it changes the route's resource and capture behavior, and the comparison could not then name the mechanism used.