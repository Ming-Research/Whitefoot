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
at eligible widths, for the kernels being timed. Five passes then rotate kernel/width order and alternate
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

The two arms are built from checkouts at
different paths, and the runtime's assert messages embed their source path, so
the linked images of unchanged code differed in every retained run, by the read-only
data after the path string. `Makefile` therefore compiles the runtime, runner and
oracle with a source-path prefix map (`PATH_MAPS`), which rewrites only embedded
path strings; code that has not changed links to the same bytes.

`judge.sh BASELINE_IMAGES CANDIDATE_IMAGES RESULTS` is the hosted judgment
(`.github/workflows/compute-regression.yml`). A kernel whose two executables are
byte-identical is the same program: it is neither verified nor timed and passes,
and when all five are identical nothing is timed. The others are timed in round 1:
a null control (the candidate's image as both arms, for the kernels being judged
only) and the comparison, each its own run-check stage, `performance-null` and
`performance-comparison`. `decide.awk` reads the two tables and gives each kernel
`pass`; `FAIL` when the comparison is adverse at two widths and the kernel's own
null control has no adverse width; or `indeterminate` when the null control also has
an adverse width, so that the host's noise on this kernel reaches the threshold. If
no kernel failed, the indeterminate kernels are measured once more alone, as
`performance-retest-null` and `performance-retest-comparison`, and decided the same
way. A kernel that failed in either round fails the job (exit 1) whatever the others
did; one still indeterminate after the retest is INCONCLUSIVE (exit 3, which the
workflow passes with a warning and a summary line, since no regression verdict is
available); a measurement that did not run refuses it (exit 2). `RESULTS/verdict.txt`
is the one record, with the kernels not timed, each round's decisions and the CPU
model; `compare.sh` also writes `cpu model:` into every `manifest.txt` and
`verdict.txt`. `reduce.awk` and `verdict.awk` take the kernel list as
`-v kernels=`, which `compare.sh` and `judge.sh` read from `KERNELS :=` in the
`Makefile`; an empty list, a repeated name or a name without rows is refused.
`test-verdict.sh` drives `judge.sh` with a stub measuring command.

Every kernel the hosted comparison times is first observed on this host against its own
image as both arms (the null control, above); a previous runner's qualification
cannot validate a new host invocation. Instrument or fixture changes additionally run an explicit
slowdown control, which repeats real WF work and intermediate checking/release
inside the candidate interval and must fail all five kernels, and a placement
control, which compares the candidate images with copies linked by
`make build PLACEMENT_PAD=96`, 96 bytes of never-executed code ahead of the
emitted module, and must pass. Such a shift moves the module and everything
after it, as one more imported library function does: by one or two whole
64-byte lines, since every compiler-produced function starts on such a
boundary, or by 32 bytes within a line if functions were not aligned. The
workflow first requires each kernel's module to have moved; a failed
comparison means placement alone would read as a regression. These controls
do not establish a statistical false-alarm rate or sensitivity to every small
regression. Failed controls retain their raw data. The only retry is the one
retest round of a kernel left indeterminate, never a changed threshold. The
null control needs no third native build.

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
