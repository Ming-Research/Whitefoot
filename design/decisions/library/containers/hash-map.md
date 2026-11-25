# Hash map

The standard library's owning HashMap: its buckets, how it probes, grows and rebuilds, what an insertion returns, and how a scan resumes.

### decision hash-map-enum-buckets-migrate-to-fresh-backing
HashMap keeps its pairs in enum buckets and rebuilds by moving only filled buckets into a fresh backing, through one local pending-owner window and the shared exchange helper.
- rejected: an agent proposed planning destinations and permuting the old backing as the default rebuild; the owner rejected it, because the direct protocol keeps owners with fewer requests and lower growth cost (data: `research/experiments/container-representation/map-library/RESULTS.md`).
- rejected: an agent proposed one inline one-slot window in every bucket; the owner rejected it, because it widens every bucket with no consistent advantage over the enum protocol in the matched rebuild comparison (data: `research/experiments/container-representation/map-library/RESULTS.md`).
- rejected: an agent proposed dense payloads as the universal representation, from wide-growth timings; the owner rejected it, because the separate index and dependent access cost differently, and no measured application makes wide growth the governing operation.
- rejected: an agent proposed duplicating the exchange inside migration to drop its intermediate result; the owner rejected it, because it showed no stable timing benefit once the unchanged C controls were counted (data: `research/experiments/container-representation/map-library/RESULTS.md`).

### decision hash-map-put-returns-one-shared-pair
A HashMap insertion returns either a success with no payload or one shared returned pair with a reason, replaced or full.
- replaced: An interface of separate Replaced and Full variants, each with its own pair, was on main; the owner changed it, because their repeated pair storage cost more for wide values (data: `research/experiments/container-representation/map-library/RESULTS.md`).
- rejected: an agent proposed inferring a compiler-known sparse representation or a universal compact-enum ABI from the result experiment; the owner rejected it, because changing the public result alone separates this cost from bucket layout, and zero-sized pairs still show tradeoffs.

### decision hash-map-probes-from-high-bit-homes
HashMap probes linearly from a key's home, the top bits of its hash after one odd multiplication, over zero or a power of two of at least two buckets, and a scan's cursor is a position in hash space.
- replaced: A home of the hash modulo the bucket count was on main; the owner changed it, because a rebuild can then move a pair not yet reported behind any physical cursor, so a scan misses pairs present throughout (data: `research/investigations/hash-scan/DESIGN.md`).
- rejected: an agent proposed home-bucket chains over a slot array; the owner rejected it, because a hit reads the chain head and then the pair: hits at a million pairs took 1.19 times probing's time, and 67% more memory at ten thousand (data: `research/investigations/hash-scan/DESIGN.md`).
- rejected: an agent proposed a link array beside the chains' slot array; the owner rejected it, because a hit past a chain's first pair reads its link on another cache line: hits were 30% and removals 20% slower than probing at a million pairs (data: `research/investigations/hash-scan/DESIGN.md`).
- rejected: an agent proposed storing each pair's hash in its bucket; the owner rejected it, because it adds a word to every bucket, a third more for `u64` pairs, while equality of such a key is one comparison.
- rejected: an agent proposed a stable-identifier directory beside the probing table; the owner rejected it, because its memory is the table's plus the directory at every size, and every insertion and removal must update the directory too.
- rejected: an agent proposed cache-line buckets with hash fingerprints and an ever-full bit, as Valkey 8.1 does; the owner rejected it, because a 64-byte bucket holds two `u64` pairs beside its metadata, more memory than plain buckets, and fingerprints save only key comparisons.
- rejected: an agent proposed keeping the first pair of each chain inline in its head; the owner rejected it, because every head then reserves a pair, so at ten thousand pairs the heads alone exceed probing's buckets, and removing a head pair moves the next one.
- rejected: an agent proposed a physical bucket cursor over linear probing; the owner rejected it, because a rebuild at the same capacity can move a pair not yet reported behind the cursor.
- rejected: an agent proposed homes from the low bits of the hash with a reverse-binary cursor, as Valkey does; the owner rejected it, because the cursor needs bit reversal across bucket-count changes, and hashes whose low bits agree all share one home.

### decision hash-map-capacity-counts-pairs
A HashMap's capacity is the pairs it holds before an insertion rebuilds it, three quarters of its buckets rounded up and at most its ceiling, and the ceiling bounds the pairs it holds.
- rejected: an agent proposed making the capacity itself a power of two; the owner rejected it, because every reservation would round up to a power of two, and growth could pass a ceiling that is not one.

### decision hash-map-rebuilds-at-three-quarters
HashMap rebuilds before an insertion once its filled and vacated buckets reach its capacity, at the same bucket count when under half of that capacity is filled, otherwise at twice the buckets.
- replaced: Rebuilding only when no bucket is left was on main; the owner changed it, because an insertion near that point probes almost the whole backing, and Firn's `INCR` at depth 16 ran 1.87 times as fast with the earlier rebuild (data: `research/investigations/firn/DESIGN.md`).
- rejected: an agent proposed counting only filled buckets toward the growth limit; the owner rejected it, because a keyspace that removes as often as it inserts fills with vacated buckets that a lookup must walk while the filled count stays low.

### decision hash-map-at-ceiling-never-rebuilds-on-insert
An insertion never rebuilds a map whose capacity has reached its ceiling, so a map created at its ceiling never allocates again; its vacated buckets stay until the program calls `hash_map_rehash`.
- rejected: an agent proposed rebuilding a map at its ceiling at the same capacity when under half the mark is filled, as below it; the owner rejected it, because that bounds the walk only while under three eighths of the buckets are filled, and makes an insertion there allocate a second backing.

### decision hash-map-scan-limit-counts-homes
A scan's limit counts home buckets, and one call lends every pair whose home it covers, hashing only the pairs of the runs at either end, so a call may lend more than limit pairs.
- rejected: an agent proposed a hard cap on the pairs one call reports; the owner rejected it, because Redis 7.2.16's COUNT is the same hint and its scan tests ask no stricter bound, while a cap needs a stable-identifier directory that changes the table more.
- rejected: an agent proposed lending every pair of the walk without hashing the runs at its ends; the owner rejected it, because a pass over an unchanged map would then lend twice the pairs of every run that crosses a call boundary.

### decision sparse-maps-use-ordinary-enum-slots
A sparse map holds its entries in the slots of an ordinary enum, and the compiler knows no projected sparse storage.
- rejected: an agent proposed compiler-known projected sparse storage; the owner rejected it, because the matched growth and refusal witnesses did not isolate enough benefit to select it (data: `research/experiments/container-representation/families/RESULTS.md`).
