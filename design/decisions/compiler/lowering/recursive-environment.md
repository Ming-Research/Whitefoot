# Recursive environment

How a group of mutually recursive functions passes the read-only borrowed parameters it forwards unchanged: one record behind an internal entry, its use, its inlining and its naming.

### decision recursive-groups-pass-borrowed-parameters-in-a-record
In a recursive group, read-only borrowed parameters every member has and every call in the group forwards unchanged travel in one `noalias readonly` record behind the public entry.
- rejected: an agent proposed a source-level record of references the writer passes; the owner rejected it, because the compiler route gives the same gain at much lower cost and changes no accepted program.
- rejected: an agent proposed only a calling convention, `x86_regcallcc` or `preserve_nonecc`; the owner rejected it, because regcall exists only on x86 and needs adapters for linked C bodies, and preserve_none clobbers the registers the matcher keeps its state in.
- rejected: an agent proposed loading every slot once at entry; the owner rejected it, because the host does not sink a load used in several blocks, so Snowghost's `instr_matches` paid four loads per call for one used slot (data: `research/investigations/recursive-shared-env/README.md`).
- rejected: an agent proposed admitting parameters written through; the owner rejected it, because a pointer loaded from the record cannot carry the `noalias` that separates those writes from every other access.
- rejected: an agent proposed admitting a parameter that some recursive edge passes as a different borrow, such as a narrowed range; the owner rejected it, because one record slot holds one value for the whole chain of activations.
- rejected: an agent proposed that every caller from outside build the record at its own call site; the owner rejected it, because the public symbol must build it anyway for C, and the host inlines that small entry into the callers it sees.

### decision the-record-is-used-only-past-the-registers
A group uses the record only when its slots replace at least two argument words and some member passes more integer argument words than the target's argument registers hold.
- rejected: an agent proposed putting every borrowed parameter of every group in the record; the owner rejected it, because a group whose arguments already fit the registers would pay a load per slot for nothing.

### decision recursive-cycle-calls-are-noinline
Inside a group that uses the record, the calls that close its cycles carry `noinline`, found as the back edges of a walk from the member called first from outside; forward calls stay inlinable.
- rejected: an agent proposed leaving every call in the group to the host's inliner; the owner rejected it, because inlining step into a rare arm of test put a callee-saved frame on every call, 16.56 instructions per call (data: `research/investigations/recursive-shared-env/README.md`).
- rejected: an agent proposed marking every call inside the group `noinline`; the owner rejected it, because the forward edges, such as compound into test, are where inlining pays, and they close no cycle.
- rejected: an agent proposed starting the walk at the first member in program order whatever calls it; the owner rejected it, because on the witness that would cut compound into test, the hot forward edge.

### decision internal-entries-are-named-by-form
An internal entry is named `wf__<form>_<name>` by its form, and a call site chooses it in one place by how caller and callee relate, naming the public symbol otherwise.
- rejected: an agent proposed a separate naming and selection path for each form; the owner rejected it, because the record form and the split-part calling-convention entry are the same mechanism, a form no other definition sees.