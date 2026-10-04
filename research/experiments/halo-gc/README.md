# Halo collector check

Checks the mark-and-sweep collector of `lib/halo/heap` (`gc.wf`; design in
[VM.md section 2](../../investigations/halo/VM.md#2-heap)) on a graph whose
every reference kind leads to one object only through it: A's array part
holds B, a node value holds the string s1, a node key is the table F; C is
alone, D and E form a cycle, s2 is alone. With A as the only root, exactly C,
D, E and s2 are freed and interning "s1" again returns the same handle; with
no root, all eight cells are freed.

```sh
whitefootc --graph research/experiments/halo-gc/modules.wfg --function pkg::test::main -o /tmp/gctest
/tmp/gctest   # exit 0 when every check holds, otherwise the failing check
```

Exit codes: 1 A freed, 2 B freed, 3 C kept, 4 D kept, 5 s1 not kept,
6 F freed, 20 wrong rooted count, 30 wrong unrooted count.

Result on 2026-10-04 (compiler from main 9a0d0af4f): exit 0. Each of three
mutants of `gc.wf`, dropping the marking of array values, of node keys or
of node values, fails with exit 2, 6 and 5 respectively.

The heap graph remains the independent reference-edge check; the VM stress
experiment below checks root enumeration. Remove the graph program when a
maintained heap test owns these same discriminating observations.

## VM stress experiment (F4)

Criterion recorded before the runs: the unchanged 80-script Redis 7.0.15
corpus must match at budgets 1, 7 and 1000 with full collection forced at
collector safepoints. Count completed collections per script, excluding the
preparation chunk. Run each missing-root mutant separately at stress and
restore it before the next mutant. A mutant that changes no corpus reply is
a coverage gap requiring a local discriminating Lua case. The restored case
must match its independently specified reply. For the 64 MiB logical heap
limit, an unbounded growing table must return an error containing
`not enough memory`, and another script on the same engine and store must
succeed afterward. These observations test VM roots and recovery, not a
complete reachability verifier or an operating-system RSS bound.

The stress instrumentation uses the existing heap trigger and sweep path;
its embedding switch is off in a new engine. The count saturates at u64
maximum and survives engine reset. Python remains fixture transport and
independent RESP2 comparison, never a Lua evaluator.

Initial sample at parent `c0cff8caed353ef0c601d407f14f47a5c888f25e`:
the native build exited 0 in 443.65 s (user 346.87 s, system 50.90 s).
`lua-core/counter-closure` at stress, budget 7, passed with 7 collections;
three direct executions exited 0 in 0.004048, 0.003436 and 0.003349 s.
This justifies the 80-script, three-budget comparison, with builds timed
separately from execution. A bounded 70-iteration growing-string table
probe exited 0 in 0.363477 s, returning 70 with 74,462,693 logical heap
bytes under a 67,108,864-byte limit. This is the negative control for the
missing memory-limit enforcement; it does not run an unbounded allocator
against the defective engine.
