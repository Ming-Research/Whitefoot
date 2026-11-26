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

## Data placement

On some hosts a kernel's time depends on where its large blocks are mapped,
which a change of startup or of the allocator moves without changing generated
code: on EPYC 7763 and 9V74 hosts `stencil` runs 26 to 43 percent slower when
its two grids lie 128 MiB less 4 KiB apart, because the first-level data
cache's way predictor cannot tell them apart
([data placement](../../research/investigations/data-placement/README.md)).
Under address randomization each launch draws where its data lies, so a
campaign of five pairs then reads such a kernel slower or not about as often.
Before the comparison, `data-placement.sh control` decides which kernels
placement decides on this host:

1. Each kernel's two arms are launched once at one worker without address
   randomization (`setarch -R`), and `strace` records their `mmap`, `munmap`
   and `mremap` calls up to the runner's `layout` exit. `layout.awk` drops the
   process ids and joins interrupted calls; sorted, the two records are equal
   when the arms map alike. A kernel that maps alike is judged as usual and is
   launched no more.
2. Both arms of a kernel that maps differently are launched at the 32
   positions: `WF_PERFORMANCE_POSITION=k` makes the runner map k times 2 MiB
   before the entry thread exists, which walks the entry's stack and heap
   through the C library's 64 MiB heap alignment. Each launch's time is the
   median of its five measured calls.
3. `positions.awk` divides each arm's 32 times by the arm's median, so a loss
   the arm has everywhere cancels, sorts them and compares the two rank by
   rank. A rank is unlike when the candidate's quotient over the baseline's is
   1.10 or more, or 1/1.10 or less. It does not read which position a time
   came from: an image that only maps more before its entry has the same
   profile moved by some positions, and address randomization draws the
   position, so such a pair has its times in equal shares and is judged.
4. A kernel with an unlike rank is launched at the 32 positions again, and is
   set aside when the second round has a rank unlike in a direction the first
   had. Its comparison rows then read `not judged (data placement)`, a
   `not judged:` line follows them, and it counts as no failure or suspect;
   the other kernels are judged as usual. The control's `verdict.txt` shows
   both arms' sorted quotients for each round of a kernel set aside, and the
   workflow shows a warning for it.

`data-placement.sh qualify CANDIDATE RESULTS`, which the workflow runs with
the other instrument controls, requires the candidate against itself to map
alike, against itself with one more idle mapping (position 1) to map
differently for every kernel, and its 32 positions launched against themselves
to set no kernel aside. At position 1 a record differs by that mapping in any
case, so each kernel's record must also differ without it: the record has to
see the later mappings lie lower, which is what shows a startup that makes the
same calls in another order. The positions are launched only when both
records pass. Each record and each kernel's round is a labeled stage
of its own (`performance-data-layout`, `performance-data-round-KERNEL`), so a
kernel added to the instrument needs its row in `.github/time-budgets.txt`.

The control reads one worker only, where the order of the program's mappings
is fixed; at two and four workers the order the workers map in is not. It
sees no effect under 10 percent (adjacent grids cost the 7763 6 to 10
percent), and only blocks the mapping calls show: not an offset of static data
within the image or of a small block within a heap. Which launches of either
arm land in a slow placement under address randomization is still noise of
the ordinary comparison, which the identical-image control bounds. One slow
launch of an arm in each round, at any positions, also sets a kernel aside:
the rule cannot tell it from placement, and the control's confirmation met it
once in 20 jobs.

## Entries and artifacts

`compare.sh BASELINE_IMAGES CANDIDATE_IMAGES FRESH_RESULTS` from the candidate
revision is the explicit measurement entry for both image sets. With
`WF_PERFORMANCE_PLACEMENT` naming the data-placement control's results, it
leaves the kernels of their `aside.txt` unjudged; it refuses a directory
without that file. `data-placement.sh control BASELINE_IMAGES CANDIDATE_IMAGES
FRESH_RESULTS` is the control's entry. Each child has a 60-second deadline, and
the workflow bounds the campaign as a whole. `manifest.txt`, `raw.tsv`,
`paired.tsv`, `verdict.txt` and per-process logs are uploaded on success or
failure, with the control's traces, mapping records, `rows.tsv` and
`verdict.txt`. The artifact also retains each arm's generated
LLVM modules, native objects and executable images, so a regression can be
inspected using the exact hosted code and its final linked addresses rather
than a local toolchain approximation. The input identity record includes the
host CPU report so architecture-dependent code-placement hypotheses can be
checked against the machine that produced the samples.
Keep these files only while the maintained regression workflow consumes them;
experimental framework comparisons belong to explicitly requested research
runs.
