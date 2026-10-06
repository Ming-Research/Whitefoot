# Paired compiler performance regression

`.github/workflows/compute-regression.yml` owns this runner. It compares the
current compiler/runtime with the merge base on the same Linux host. Neither
`make check` nor ordinary correctness CI constructs a baseline or runs timing.
Their only performance-instrument check is `sh tests/performance/test-verdict.sh`,
which uses small synthetic tables and no compiler.

The five programs and independent complete-result oracles live in
`tests/programs/compute/`: Mandelbrot, UTF-8 records, FIR, adaptive quadrature
and stencil. `Makefile` uses the current timing runner to compile each
revision's corresponding formal source, host adapter and independent oracle
with that revision's compiler and ordinary runtime. This compares the same
workload across a language port without requiring the older compiler to parse
newer source syntax or the newer source to use a retired native ABI. Before
construction, the workflow requires the two revisions to name the same kernel
set, declare identical `wf_oracle_name` and `wf_oracle_fixture` identities,
and expose the same oracle interface. A mismatch or a build failure is missing
comparison evidence, never a passing performance result. No historical source
adapter or research-side baseline discovery is used. These identity checks
guard selection and ABI compatibility; review of the source and independent
oracle ports establishes that they still implement the same algorithm and
inputs, since matching identity strings alone cannot prove semantic
equivalence.

Each arm has five native images. Both arms first run their correctness matrices
at eligible widths. Five passes then rotate kernel/width order and alternate
the arm order. One process contributes an unrecorded warmup and five measured
calls. Fixture construction, reference answers, checking, final release,
printing and shutdown are outside timing; output construction and all joined
WF work are inside. Process CPU includes all threads. The reducer takes the
median of each process's five calls, then the median of the five paired
baseline/candidate ratios. It rejects missing or duplicate cells, arms,
passes and samples. Decisions use full precision, not displayed rounded text.

Only widths 1, 2 and 4 that fit the host's available CPU count participate.
Fewer than two available CPUs refuses the run. On a four-CPU host, a campaign
has 150 measured processes and 30 preceding correctness processes. A width is
adverse when its wall ratio is below 0.97 and the baseline is faster in at
least four of five pairs. Two adverse widths in one kernel fail; one is a
visible suspect. This can miss a real regression confined to one width.
CPU ratios below 0.90 are reported using the wall pair count and never fail.
These thresholds are a noise/detection tradeoff, not a universal guarantee.

Every hosted comparison first runs the identical candidate images as both
arms. This control must produce no failing kernel; otherwise the job fails
with inconclusive measurement evidence and does not run the compiler
comparison. A previous runner's qualification cannot validate a new host
invocation. Instrument or fixture changes additionally run an explicit
slowdown control, which repeats real WF work and intermediate checking/release
inside the candidate interval and must fail all five kernels, and a placement
control, which compares the candidate images with copies linked by
`make build PLACEMENT_PAD=32`, 32 bytes of never-executed code ahead of the
emitted module, and must pass. Such a shift moves every function after it, as
one more imported library function does; since every compiler-produced
function starts on a 64-byte boundary, it moves them by whole lines, and a
failure means placement alone would read as a regression. These controls
do not establish a statistical false-alarm rate or sensitivity to every small
regression. Failed controls retain their raw data; there is no automatic retry
or threshold adjustment. The identical-image control needs no third native
build.

`compare.sh BASELINE_IMAGES CANDIDATE_IMAGES FRESH_RESULTS` from the candidate
revision is the explicit measurement entry for both image sets. Each child has
a 60-second deadline, and the workflow bounds the campaign as a whole.
`manifest.txt`, `raw.tsv`, `paired.tsv`, `verdict.txt` and per-process logs are
uploaded on success or failure. The artifact also retains each arm's generated
LLVM modules, native objects and executable images, so a regression can be
inspected using the exact hosted code and its final linked addresses rather
than a local toolchain approximation. The input identity record includes the
host CPU report so architecture-dependent code-placement hypotheses can be
checked against the machine that produced the samples.
Keep these files only while the maintained regression workflow consumes them;
experimental framework comparisons belong to explicitly requested research
runs.
