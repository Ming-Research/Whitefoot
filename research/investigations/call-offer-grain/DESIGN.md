# Call-offer grain under `--par`

## Question

Under `--par` the compiler offers independent calls to idle workers: every
member of a permitted PAR-1 statement group but the last is published to the
calling lane's deque, and the halves of a PAR-2 range split are published the
same way ([parallel lowering](../../../design/compiler/parallel-lowering.md),
[parallel runtime](../../../design/compiler/parallel-lowering/parallel-runtime.md)).
A range split is priced: the splitter asks the runtime how many chunks the
span affords at a 150,000-unit work floor. A call offer is not priced. The
only filter is the scalar-leaf limit, which drops offers of one-block
functions over scalar values with at most 16 operations, and the tree says
that "no universal grain policy is selected, because one exploratory result
on one workload supports the provisional default".

The first large real program built with `--par`, Snowghost's style prototype,
is swamped by call offers. Its page setup parses the 7.6 MB ECMAScript
specification with `pkg::html::tree_builder` in 0.30 s at one worker and
17.5 s at four, about 18 million steals, as its author recorded in the
prototype's `run.sh`.

What grain policy should `--par` use for call offers, so that ordinary real
programs never get slower with more workers, while the maintained parallel
programs keep their measured speedups?

## Selection criterion

Recorded before any candidate policy was built or measured. A candidate is
selectable only when it meets all three conditions on this host, in the same
session as the baseline it is compared with, and produces the same results:
the prototype's `check` mode passes on every page and every maintained program
passes its own result check.

1. **No real page sets up slower with more workers.** For each of the three
   real pages of the prototype's dossier (ecma262, html5, apollo11), the
   candidate's `--par` build has a best-of-seven setup time T(0) at
   `WF_WORKERS=2` and at `WF_WORKERS=4` no greater than its own best-of-seven
   T(0) at `WF_WORKERS=1` plus the noise allowance: 5 percent of the
   one-worker time or 10 ms, whichever is larger. On this shared host the
   sequential and one-worker best-of-seven times of the
   [par-quicksort record](../../experiments/par-quicksort/README.md#results)
   repeat within 2 percent between batches and the four-worker time within
   6 percent; the configurations compared here run interleaved in one block,
   which removes the batch term, and 10 ms covers process start and page
   reading on the fastest page.
2. **The maintained parallel programs keep their times.** A program whose
   candidate `--par` emission (`--emit-llvm`) and linked runtime are
   byte-identical to the baseline's is unchanged and needs no timing. Otherwise:
   - the five formal kernels of [`tests/performance`](../../../tests/performance/README.md)
     (Mandelbrot, records, FIR, quadrature, stencil) pass that runner's own
     rule against the baseline images (no kernel with two adverse widths, a
     width being adverse when its paired wall ratio is below 0.97 with the
     baseline faster in at least four of five pairs), after the same
     session's identical-image control passes;
   - [par-quicksort](../../experiments/par-quicksort/README.md) and every
     other program under `tests/programs` whose emission changes have a
     best-of-seven time at `WF_WORKERS=1`, 2 and 4 within 5 percent or 10 ms,
     whichever is larger, of the baseline's best of seven in the same block.
     Each such block also times a second copy of the baseline image as an
     identical-image control; a block whose control differs from the
     baseline by more than the same allowance is inconclusive and is rerun.
3. **Lowering cost stays within noise.** The front-end time `--report` prints
   for the style prototype's `--par` build, the largest program at hand,
   has a best of seven within 5 percent of the baseline compiler's best of
   seven.

When more than one candidate meets all three, prefer the one that adds no
work to an offer at run time, then the one stated over fewer program
properties, then the one with the lower worst four-worker to one-worker setup
ratio over the three pages. When none meets all three, no policy is
recommended as a default and the result names what each candidate lacks.

Blocks are timed under the shared verification lock. The one-minute load is
recorded with `uptime` before and after every block; the reading before the
block, taken once this investigation's previous block has decayed, is the
background load, and a block that starts above 2.0 is rerun.
