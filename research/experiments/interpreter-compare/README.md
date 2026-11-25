# Two-source interpreter comparison

This manual experiment answers one question for a change to the wasm
interpreter v2h (`research/experiments/match-dispatch/wasm`): does the new
source run CoreMark faster or slower than the old, with the same compiler, and
which instructions' handlers take the time? `.github/workflows/
interpreter-compare.yml` runs it on request, on the owner's i9-14900K or on a
hosted runner; it is never a gate. `compiler-compare` answers the other
question, one program under two compilers; keep these files while an
interpreter change still needs this comparison, and remove them with the
workflow when none does.

## What is compared

The candidate and the base are two commits. Each commit's own
`research/experiments/match-dispatch/wasm` (the generator and its templates)
writes its interpreter source; the candidate commit's compiler builds both
sources, so that the arms differ in the interpreter source alone. The base
defaults to the candidate's merge base with `main`. The same commit twice is
the null run: it shows only this run's noise. A commit that has no generator
fails the build, naming the commit.

The CoreMark module is the pinned Silverfir-nano `coremark.wasm` (blob
`70b4d2165e7f`, checked). The images are the base, a twin (a byte copy of the
base image, whose difference from the base measures nothing but this run's
noise) and the candidate. `interp.py` of `compiler-compare` builds and times
them: its launch, its checks of the CoreMark CRCs, its rule and its self-test
are used as they are.

| Mode | What it does |
| --- | --- |
| `time` | Rounds that run the three images once each in a rotating order, pinned to one CPU with `taskset`, after one untimed run of each. The summary prints the candidate/base and twin/base ratios with their 95% intervals, the twin's noise, and the wall time's residual over user+sys CPU time. Fewer than 6 rounds is a trial: it prints the ratios, no interval and no verdict. |
| `sample` | One `perf record -e cpu-clock -F 20000` of CoreMark per source, pinned. Each sample is assigned to the handler whose body symbol (`wf_run.body.arm.K`, K the arm's place in the source's `run` loop) holds its address. The table lists every handler of either source with its share of all samples, then the rows for samples outside every handler, and the difference of the two sources. |

## Criterion

The decision rule is `compiler-compare`'s (the hash-map comparison's paired
rule, with the 2% resolution) applied to the one workload; see its README for
the rule and the floor. A run given no floor measures it and prints a `floor`
line; `floor` takes `coremark=SPREAD` from such a line.

Every CoreMark launch checks the list, matrix and state CRCs of the 2K
performance run. The final CRC has an independent value at 2000 iterations
(0x4983) and at 20000 (0x382f); at another count only the agreement of all
launches of the run is checked, so a count other than these two leaves a source
that computes a wrong final CRC undetected when both sources compute the same
wrong one. Use 2000 or 20000 for a result.

The residual is each run's wall time minus the user and sys CPU time the
kernel accounted to its processes. The workload is pinned and CPU bound, so the
two agree to the process's start-up. A wall time on a coarse grid does not:
`residual_limit` makes a workload whose median residual is above it invalid
data, and the limit is chosen from a measurement (RESULTS.md), not assumed.

## Controls

The twin is the null control in every `time` run. `sample` has none of its own:
two runs of one source are the only measure of its sampling noise, and the
shares are not judged. `interp.py self-test` and `handler_samples.py
self-test` run in the `selftest` stage: the first covers the launch (a failing
`timeout` first on `PATH` fails any launch that goes through it; a 0.21 s
sleeper must read as its own time; a run past its bound is killed), the
residual check and the round limits; the second the symbol mapping, the
ambiguous addresses and the rows for samples outside the handlers.

## Running and outputs

```sh
gh workflow run interpreter-compare.yml --ref main -f candidate=BRANCH
gh workflow run interpreter-compare.yml --ref main -f candidate=BRANCH -f mode=sample
```

`rounds`, `iterations`, `cpu`, `floor` and `residual_limit` are inputs. The job
summary holds the run's summary; the artifact holds the manifest (resolved
commits, compiler, CPU), `time.tsv` (every run's wall, user and sys time),
`handler-shares.tsv` and the perf data. On a hosted runner the workflow
installs perf and lowers `kernel.perf_event_paranoid`; a hosted runner shares
its host, so its runs show the instrument working and decide nothing.

## Limits

One program, one input size and one CPU. The wall time is that of the whole
process, start-up included, which is the same for the arms. A run's noise floor
holds only for the launcher, image kind, CPU and iteration count it was
measured with (RESULTS.md). `sample` counts every sample of the process: at 20
iterations the hosted trial put two thirds of the samples under `[unknown]`
(RESULTS.md), so take shares from runs of 2000 iterations or more and read that
row first.
