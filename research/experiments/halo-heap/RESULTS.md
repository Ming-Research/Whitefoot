# Halo E1 results

Measured on 2026-10-04. The [criterion fixed before measurement](../../investigations/halo/DESIGN.md#planned-experiments) is Whitefoot at most 3 times C time at depths 14–16 and faster than Redis Lua. It is **met at every measured depth**. This establishes viability for this binary-trees handle-heap workload under these conditions; it does not select Halo's final heap design.

## Wall time

Seconds from `/usr/bin/time -l`, three fresh-process runs per point. Min–max and median use all three runs; probes are excluded. [README.md](README.md#run-and-measure) gives commands, ordering, output validation and the unchanged repetition rule.

| Depth | Program | Run 1 | Run 2 | Run 3 | Median | Min–max |
|---|---|---:|---:|---:|---:|---:|
| 14 | C | 0.08 | 0.08 | 0.08 | 0.08 | 0.08–0.08 |
| 14 | Whitefoot | 0.06 | 0.06 | 0.06 | 0.06 | 0.06–0.06 |
| 14 | Redis-Lua | 0.53 | 0.52 | 0.52 | 0.52 | 0.52–0.53 |
| 15 | C | 0.15 | 0.16 | 0.15 | 0.15 | 0.15–0.16 |
| 15 | Whitefoot | 0.14 | 0.14 | 0.14 | 0.14 | 0.14–0.14 |
| 15 | Redis-Lua | 1.06 | 1.05 | 1.06 | 1.06 | 1.05–1.06 |
| 16 | C | 0.38 | 0.39 | 0.37 | 0.38 | 0.37–0.39 |
| 16 | Whitefoot | 0.37 | 0.37 | 0.34 | 0.37 | 0.34–0.37 |
| 16 | Redis-Lua | 2.55 | 2.57 | 2.54 | 2.55 | 2.54–2.57 |

| Depth | Whitefoot/C | Whitefoot/Lua | Verdict |
|---|---:|---:|---|
| 14 | 0.750× | 0.115× | Meets both conditions |
| 15 | 0.933× | 0.132× | Meets both conditions |
| 16 | 0.974× | 0.145× | Meets both conditions |

The largest measured spread was Whitefoot at depth 16: 8.11%. No point exceeded 10%, so no additional repetitions were taken. Even Whitefoot's maximum divided by C's minimum is at most 1 at each depth, and its maximum is below Lua's minimum. The coarse timer and shared-host load do not support a precise claim about the small C/Whitefoot difference. No result exceeded 3 times C, so E1's conditional handle-check attribution was not triggered.

## Peak resident memory at depth 16

These are per-process peaks from `/usr/bin/time -l`, not cumulative allocation counts or the slab's reserved virtual capacity. Bytes are retained to make the conversion inspectable.

| Program | Run 1 bytes | Run 2 bytes | Run 3 bytes | Median MiB | Min–max MiB |
|---|---:|---:|---:|---:|---:|
| C | 5586944 | 5570560 | 5554176 | 5.31 | 5.30–5.33 |
| Whitefoot | 33062912 | 33079296 | 33079296 | 31.55 | 31.53–31.55 |
| Redis-Lua | 82968576 | 82968576 | 82919424 | 79.12 | 79.08–79.12 |

Whitefoot's median peak RSS is 5.94 times C's and 0.399 times Lua's. The timing criterion does not impose an RSS threshold; the larger resident footprint versus C remains a limitation.

## Machine, load and toolchain

- Apple M1 Pro, MacBookPro18,3, 8 logical CPUs, 34359738368 bytes RAM (32 GiB), arm64. macOS 26.6.2, build 25G83, Darwin 25.6.0.
- Apple clang 21.0.0 (`clang-2100.3.34.2`). C compiled with `cc -O2`; Redis Lua rebuilt from the supplied Redis 7.0.15 sources with `-O2 -Wall -DLUA_USE_POSIX`; standalone interpreter reports Lua 5.1.5.
- Supplied `gate/whitefootc`, specification v0.89, sequential default native build. Compiler checkout HEAD was `db07796a985d53dcf5166542762e42fd822dea49`, clean when inspected. The executable predates that commit; comparison with the preceding `7eee5cf13` revision found only a compiler test change under `compiler/`, and no change under `compiler/src`, `compiler/build.rs`, `lib` or `spec`. The executable has no embedded git revision, so exact build-commit provenance is unverified. Its SHA-256 is `7ba82d6cf131cbfab5d83c7f29dc4a3c4a3c51227d9437ac80d5b578d4719922`; embedded build identity is v0.89 / specification SHA-256 `f629ee44416828db06f6208550fcb8b7a2941ef7583bc0a1e87f5550f31b3eee`. No Cargo build was run.
- Experiment checkout base: `cc632f741162b922e9b5ac01698c511ee3bc9b20`. Source SHA-256: `heap.wf` `13a9adba1a1716cc27659fc1464b6ec14055e59878c1d432e36dbdcb080a259c`; C `fbe617fa9f463428fbd4a2989205cfd70cea111690a29a2c092e03d10bb30a2b`; Lua `3ee519da7cad7666e8b2ea4259d4a5b2d2cb894c81b89ee7117bc6ff203bf93d`.

Other heavy jobs were running on this host. The shared verification lock serialized cooperating jobs; it did not make the machine idle. Measured invocations ran around 02:56 PDT. `uptime` readings immediately before them included:

```text
2:56 up 29 days, 12:38, 3 users, load averages: 11.67 11.19 9.54
2:56 up 29 days, 12:39, 3 users, load averages: 10.82 11.02 9.49
2:56 up 29 days, 12:39, 3 users, load averages: 9.69 10.78 9.42
2:56 up 29 days, 12:39, 3 users, load averages: 9.40 10.70 9.40
2:56 up 29 days, 12:39, 3 users, load averages: 9.42 10.66 9.40
2:56 up 29 days, 12:39, 3 users, load averages: 15.96 11.99 9.87
2:56 up 29 days, 12:39, 3 users, load averages: 22.93 13.50 10.42
```

The one-/five-/fifteen-minute load ranges during measured runs were 9.40–22.93 / 10.66–13.50 / 9.40–10.42. A later documentation-time reading was 38.98 / 18.84 / 12.50, illustrating continued host activity. No samples were discarded for load, and no timing was corrected for it.

## Validation and limitations

The successful Whitefoot build took 1.15 s (`time` real), C build 0.09 s, and the clean successful Lua build 2.56 s. These are separate from execution measurements. All three programs matched the independent complete-tree formula at depth 4, the depth-14 probe, and all measured runs at depths 14–16, with zero exit status. Compiler-required syntax and proof guards were adapted without changing the algorithm or criterion. Repository compiler/gate suites are outside this research-only task and were not run.

A separate read-only GPT-6 review covered all five experiment files against base `cc632f741162b922e9b5ac01698c511ee3bc9b20`, checklist groups A/D/M/V and evidence item R2, including design correspondence. It independently checked the collector, all 27 raw output/time records, aggregates, load ranges, source and toolchain hashes, and compiler-source comparison; no benchmark or green suite was rerun. One claim was repaired from “below 1” to “at most 1” for maximum Whitefoot/minimum C time (depth 16 is exactly 1). No findings remain within scope. Negative controls rejected changed counts, missing lines and nonzero exits; explicit whitespace checks of every untracked file passed and a trailing-space control failed. No design-tree or specification change, approval decision or PR publication is part of this directory-only experiment.

- Collection is stop-the-world and restricted to **safe points between tree constructions**, after any transient tree has been checked and discarded. The long-lived tree is in an explicit root list while needed. No collection occurs inside recursive construction or checking, so stack-held partial trees need no tracing. An allocation burst can exceed the threshold before the next safe point; this is not an arbitrary-allocation collector for a running Lua interpreter.
- The slab reserves 1048576 slots once and does not grow. Sweep scans all materialized cells, including vacant ones; backing capacity is retained until exit. The root list holds two Values and the iterative worklist holds 64; failure to fit or an invalid handle returns failure. These capacities cover this workload, not arbitrary graphs. Sharing and cycles are not exercised.
- Marking uses a monotonically increasing epoch and refuses epoch exhaustion; marks need no clearing. The generational slab reuses reclaimed slots. Construction/checking still use recursion, bounded by the workload's maximum tree depth of 17.
- Count and power arithmetic uses explicit wrapping operations where the full generic proof would add unrelated obligations. For inputs 4–16, the independent formula establishes that actual counts and allocation volume fit u64; all printed values were checked against it. This is a workload bound, not an unbounded interpreter claim.
- C frees promptly; Whitefoot performs batch tracing/sweeping; Lua uses its default collector. Their allocation and reclamation policies intentionally differ as E1 specifies. Whitefoot nodes carry tagged handles and a mark, C nodes two pointers, and Lua nodes tables. This experiment does not isolate each representation or collector cost.
- Measurements are short (0.06–2.57 s), with 0.01-second timer resolution, one machine, three runs per point and variable competing load. They support the wide separation from the 3× limit and from Lua, not a general Whitefoot/C performance ranking. Peak RSS includes process/runtime overhead and differs from live object bytes.

Found along the way: the copied Redis Lua build requires its relative compatibility header (fixed in scratch build layout); macOS `time -l` requires kernel-query access beyond the sandbox (measurements ran with that access); the supplied compiler lacks exact embedded build-commit provenance (recorded above). No compiler, specification, library or design-tree change was made or selected.
