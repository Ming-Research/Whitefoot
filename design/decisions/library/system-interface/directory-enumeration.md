# Directory enumeration

How a program lists a directory: where the records go, what a name is, which entries a listing leaves out, and what a batch returns.

### decision directory-batch-fills-caller-storage
`directory_next` transfers a bounded batch of records into a range of byte storage the caller owns and advances the directory cursor it is given.
- rejected: an agent proposed a window owned by the directory handle, drained one item at a time; the owner rejected it, because it needs a second draining interface and lifetime rules for the entries it hands out.

### decision listed-names-are-bytes-in-no-order
A listed name reaches source only as name bytes, and a listing fixes no order.
- rejected: an agent proposed ordered entries valued as paths; the owner rejected it, because a name as a host string or path would bring back a path model, and a fixed order would promise what hosts do not give.

### decision listing-leaves-out-self-and-parent
A listing leaves out the self and parent entries, and a successful batch over a nonempty window holds an entry, the host operation reading again when a host batch held only those two.
- rejected: an agent proposed reporting the self and parent entries unfiltered; the owner rejected it, because every program must then skip them in the host's encoding, which the walk in `tests/programs/wfgrep.wf` did for one-byte names only, so it opened them on Windows.
- rejected: an agent proposed a successful batch with no entry when the host's batch held only the self and parent entries; the owner rejected it, because it reports progress without any, so a caller that ends at an empty batch misses the later entries.
- rejected: an agent proposed submitting the second read from the operation's finish; the owner rejected it, because the finish would then wait on a host read its frame does not suspend for, stopping every context its driver runs.

### decision directory-batch-returns-separate-results
A directory batch returns its outcome, its numeric endpoint and its entry count as separate results, the endpoint's bounds holding unconditionally.
- rejected: an agent proposed a proof producer made for directories; the owner rejected it, because a numeric result carries its bound as an ordinary comparison the caller keeps after the call.
- rejected: an agent proposed returning the bound inside a success variant's payload, with a bounds-failure branch; the owner rejected it, because the caller would reach the bound only through a later match, on a branch the operation can never take.