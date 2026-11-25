# Wide probe

The fast path the lowering gives a loop that walks bytes: how it is recognized and which runs its probe reads.

### decision a-byte-walk-loop-gets-a-wide-probe-fast-path
The lowering recognizes a byte-walk loop on its checked statements alone and gives its header a probe of how many next iterations have no effect, else lowers it as usual.
- rejected: an agent proposed restructuring the scalar loop so the backend's own vectorizer widens it; the owner rejected it, because a preregistered C control on the same compiler got no early-exit vectorization, and guarded per-byte loads never fused into wide loads.

### decision the-byte-probe-reads-inline-and-boxed-arrays-alike
The byte probe is one operation over both `Array` placements, reading each run's element base and either its constant length or the length in its runtime header.
- replaced: Matching only a runtime-capacity run with an empty root path was on main; the owner changed it, because both `Array` placements are contiguous runs and the checked place already tells their representations apart.
- rejected: an agent proposed keeping the probe on the boxed form alone; the owner rejected it, because both placements are contiguous runs, and the inline array is the form every stack-resident scan uses.
- rejected: an agent proposed building a pointer-and-count descriptor for every operand; the owner rejected it, because the checked complete place already distinguishes the two representations, and the probe needs only a length and an element base.
- rejected: an agent proposed extending the probe to `Slots` and `Ring`; the owner rejected it, because no workload selects them, and a `Ring` has a head and may wrap.