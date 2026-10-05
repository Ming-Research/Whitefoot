# Halo P1 performance baseline

Question: does the current Halo match dispatch meet the P1 median thresholds
in [VM.md sections 4, 5, 10 and 11](../../investigations/halo/VM.md)?
Criterion fixed before measurement: each Halo median must be at most Redis
7.0.15's bundled PUC Lua 5.1.5 median. A ratio above 1.5 on fib or the
numeric loop triggers attribution, inspecting value width and handle checks
first. Profile every measured kernel above 1.5; report sampled work in
value/handle access, dispatch, slow execution, GC and budget handling. Sampling
cannot establish the causal speedup of removing that work. The initial
baseline selected no C1–C6 candidate; subsequent bounded same-source trials
are recorded in [RESULTS.md](RESULTS.md). The later cost-repair experiment
below evaluates embedding roots
and the collector predicate separately from those candidates.

The seven `kernels/*.lua` are standalone Lua 5.1 scripts printing one numeric
checksum. Integer-table fills and then reads every entry. String-key rotates
four present keys; concat rotates four short interned results. Sort uses the
Park–Miller generator (seed 42, multiplier 48271, modulus 2147483647), whose
integer products remain exact in binary64, and checks order before summing.
Binary-trees uses item/left/right tables, a stretch tree, a retained tree,
and paired trees at each even depth. These files and the host serve P1;
remove them when this baseline is superseded without needing reproduction.

The native `host/` installs only numeric `print`, uses `pkg::embed` with a
2 GiB logical live-heap limit, normal GC, and budget 2^64−1 (the VM’s unlimited sentinel, which skips decrements).
The budget-1000 comparison includes counter updates and embedding resume work;
it does not isolate decrement cost. Its stderr is
two numeric lines: suspension count and collection count. Any positional
argument selects budget 1000 (the oracle corpus default). `run.py` drives
existing native tools and validates their independently printed outputs;
Python does not implement compiler or VM semantics. This runner stays while
P1 is reproducible and is removed with the experiment if retired.

Build with the existing compiler and full runtime LTO (no Cargo or network):

```sh
perl .github/run-check.pl halo-bench-build compiler/target/gate/whitefootc --graph research/experiments/halo-bench/modules.wfg --entry bench --full-lto -o research/experiments/halo-bench/target/halo
```

Use `target/` for regenerable logs and profiler reports. Run the smallest
sample first (`--kernels fib --runs 1`), then three interleaved pairs; inspect
spread before choosing repetitions. The runner never launches a full matrix
by default. Repeat the one/three calibration for each workload before a
selected batch. `--reference-only` sizes a PUC workload before risking a long
Halo launch; those timings are calibration, not a checksum-verified baseline. If PUC is far above roughly two seconds, lower its `N` with
`--scale`; the replacement applies to identical bytes on both engines and is
recorded. `--budget realistic` measures Halo at 1000 instead of 2^64−1.

```sh
perl .github/run-check.pl halo-bench-sample python3 research/experiments/halo-bench/run.py --lua /path/to/redis/deps/lua/src/lua --kernels fib --runs 1 --out research/experiments/halo-bench/target/fib-one.json
```

Wall time surrounds each process launch through exit (source reading, engine
creation/compilation, execution, collection and teardown included). PUC reads
the same stdin bytes with `loadstring` and executes them; alternate PUC/Halo
and Halo/PUC pair order to balance drift. Checksum disagreement, nonzero exit,
missing/extra output, malformed stats, or any suspension in the large-budget
mode fails the run before its timing is admitted. Store raw observations in
JSON, including launch order, outputs, exit codes, source/binary/compiler
hashes, source revision, host and scales. Spread means min–max, with relative
range `(max−min)/median` also reported. Six pairs are the minimum selected
baseline; lengthen only if spread affects a threshold conclusion.

For attribution on macOS, `run.py --profile` samples each individual Halo
launch at 1 ms with `/usr/bin/sample`; its result JSON records the profiler's
exit and report. Use the same workload and budget as the baseline, and
exclude profiled timings from baseline medians. The runner requires one
nonzero sampled execution worker, including when
`sample` itself exits 0. Short processes may finish before attachment;
report this limitation or choose a longer recorded profiling workload. Raw
data and supported attribution are in [RESULTS.md](RESULTS.md).

## Same-source Halo cost pairs

`--before-binary PATH` alternates Before/Halo then Halo/Before launches,
feeding identical source bytes to both. A separate PUC launch supplies the
independent checksum; both native outputs and stats must validate, and their
collection counts must agree. `--before-budget large|realistic` selects the
first binary's budget independently of `--budget` for the second. Before/after
pairs cannot be profiled or reference-only. `before` replaces `puc` in this
mode's timing summaries; the PUC check is not a timed pair member.

```sh
perl .github/run-check.pl halo-repair-pair python3 -B research/experiments/halo-bench/run.py --lua /path/to/redis/deps/lua/src/lua --before-binary research/experiments/halo-bench/target/halo-before --binary research/experiments/halo-bench/target/halo-roots --before-budget realistic --budget realistic --kernels loop --runs 1 --out research/experiments/halo-bench/target/roots-one.json
```

The runner and retained cost observations serve the P1 cost-repair criteria in
RESULTS.md and are removed when those comparisons no longer need reproduction.
