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

Remove this program when the VM's own tests force collection across the
oracle corpus (VM.md, F4).
