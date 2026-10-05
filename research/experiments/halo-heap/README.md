# Halo E1: the heap

Does a heap of generational slab handles with a stop-the-world mark-and-sweep collector cost too much for Lua-shaped allocation? This is [Halo's planned experiment E1](../../investigations/halo/DESIGN.md#planned-experiments): compare binary-trees in C with `malloc`/`free`, Whitefoot over the handle heap, and Redis 7.0.15's own Lua 5.1. The unchanged criterion is Whitefoot time at most 3 times C time at depths 14–16, and less time than Redis Lua; above 3 times, attribute the cost of each handle check before proceeding with the design. These programs and their dated evidence belong here while E1 is referenced; retire them if superseded by an equivalent experiment.

## Programs

- `binarytrees.c`: recursively allocate a two-pointer node per tree node, recursively count nodes, and recursively free each discarded tree.
- `binarytrees.lua`: recursively build tables `{left, right}` (empty tables at leaves), count nodes, and let Redis Lua's default garbage collector reclaim discarded trees. No GC tuning or explicit `collectgarbage` call.
- `heap.wf`: `Value::Nil()` or `Value::Node(h: SlabHandle)`, with `TreeNode` objects containing two Values and an epoch mark in `std::collections::slab::Slab`. Construction uses `slab_insert`; checking uses `slab_visit`; marking uses `slab_edit`; sweeping reads occupied cells and calls `slab_remove` for stale marks. The collector starts from an explicit root list and uses a depth-first worklist rather than recursive marking. It collects at safe points when allocations since collection exceed `max(live after last collection, 65536)`.

All accept maximum depth N in 4–16. They check a stretch tree of depth N+1, retain a tree of depth N, then build/check/discard `2^(N-d+4)` trees at each even depth d from 4 through N. Finally they check the retained tree. Build and check recursion reaches at most depth 17. The same check lines are printed; see [RESULTS.md](RESULTS.md) for measurements and limitations.

## Build

Run from the repository root on macOS with the supplied Whitefoot compiler, Redis source checkout, `cc`, `make`, Python 3, and `/usr/bin/time`. All generated files stay in scratch directories. Do not build the Whitefoot compiler with Cargo.

```sh
WF_COMPILER=/private/tmp/wf-firn-batch2/compiler/target/gate/whitefootc
REDIS_SOURCE=/private/tmp/wf-redis-7.0.15
SCRATCH=/private/tmp/halo-e1-build
LUA_SCRATCH=/private/tmp/halo-e1-lua
mkdir -p "$SCRATCH" "$LUA_SCRATCH/redis/deps" "$LUA_SCRATCH/redis/src"

perl .github/run-check.pl halo-e1-c-build \
  cc -O2 research/experiments/halo-heap/binarytrees.c -o "$SCRATCH/binarytrees-c"
perl .github/run-check.pl halo-e1-wf-build \
  "$WF_COMPILER" --full-lto research/experiments/halo-heap/heap.wf -o "$SCRATCH/heap"

cp -R "$REDIS_SOURCE/deps/lua" "$LUA_SCRATCH/redis/deps/lua"
cp "$REDIS_SOURCE/src/solarisfixes.h" "$LUA_SCRATCH/redis/src/solarisfixes.h"
make -C "$LUA_SCRATCH/redis/deps/lua/src" clean
perl .github/run-check.pl halo-e1-lua-build \
  make -C "$LUA_SCRATCH/redis/deps/lua/src" lua MYCFLAGS=-DLUA_USE_POSIX
"$LUA_SCRATCH/redis/deps/lua/src/lua" -v
```

The measured heap executable uses `--full-lto`. For correctness-only incremental
builds, replace that flag with
`--cache "${WHITEFOOT_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/whitefoot}/halo-heap"`.
The cache persists outside the repository. Omit the cache flag to disable it;
`--no-cache` is a runner option, not a compiler option. Full LTO and caching
cannot be combined; cached runtime differences remain unmeasured.

Use fresh scratch directories when reproducing the copy commands. Redis's `lua_cjson.c` includes `../../../src/solarisfixes.h`, so the copy preserves that relative layout and includes the header. A flat copy failed at that include; the successful build above compiles the original Redis Lua sources without changes, with `-O2 -Wall -DLUA_USE_POSIX`. The interpreter reports Lua 5.1.5. The system/Homebrew Lua 5.4 is not used.

## Run and measure

These are the individual invocations, with depth 14 as the first timing probe. Substitute 15 or 16 only after that probe establishes a short runtime.

```sh
perl .github/run-check.pl halo-e1-run \
  /usr/bin/time -l "$SCRATCH/binarytrees-c" 14
perl .github/run-check.pl halo-e1-run \
  /usr/bin/time -l "$SCRATCH/heap" 14
perl .github/run-check.pl halo-e1-run \
  /usr/bin/time -l "$LUA_SCRATCH/redis/deps/lua/src/lua" \
  research/experiments/halo-heap/binarytrees.lua 14
```

For the recorded measurement, a one-shot Python orchestrator in scratch ran those exact `/usr/bin/time -l` child commands using `subprocess.run(..., text=True, capture_output=True)`. It read each child's exit status directly, saved stdout and time stderr in scratch, and read `uptime` immediately before each child. The repository's `run-check.pl` held the shared verification lock around each batch. On this macOS host, `time -l` needs kernel-query access outside the execution sandbox.

After a correctness sample at N=4 and one sizing probe at N=14, each depth 14, 15, 16 received three measured runs of each program. Order rotated by round: C/Whitefoot/Lua, Whitefoot/Lua/C, Lua/C/Whitefoot. Each invocation was a fresh process. Times are the `real` field of `/usr/bin/time -l`, in seconds at its 0.01-second printed resolution, not Python elapsed time or the wrapper's batch duration. They include startup, the algorithm, check-line output and final process cleanup; exclude builds. Output was captured consistently for every program. RSS is `maximum resident set size` from the same command, in bytes on macOS, converted to MiB by dividing by 1048576.

The median and min–max include all three measured runs, excluding probes. Ratios divide program medians. Spread is `(maximum - minimum) / median`; only a spread exceeding 10% permits up to two additional runs for that program/depth. None exceeded it here, so there are exactly three measured runs per point. No affinity, scheduler priority, driver-count override, GC tuning or load correction was applied.

Every captured stdout was compared byte for byte to this independent expectation, and every process had to exit successfully:

```python
def expected(n):
    lines = [f"stretch tree of depth {n+1}\t check: {2**(n+2)-1}\n"]
    for d in range(4, n+1, 2):
        iterations = 2**(n-d+4)
        lines.append(f"{iterations}\t trees of depth {d}\t check: "
                     f"{iterations * (2**(d+1)-1)}\n")
    lines.append(f"long lived tree of depth {n}\t check: {2**(n+1)-1}\n")
    return "".join(lines)
```

The oracle rejects altered node counts, missing output lines, and unsuccessful process exits. At N=16 each program must print stretch check 262143, depth checks 2031616, 2080768, 2093056, 2096128, 2096896, 2097088, 2097136, and retained-tree check 131071. This observes both transient trees and survival of the retained tree across repeated collections; the total allocation volume also exceeds the slab's capacity, requiring reclamation and slot reuse.
