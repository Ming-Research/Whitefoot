# Halo compiler comparison results

Tested commit: `5391dc7b1b3f256bf415a924d08901da39054b8c`.
Worktree base: `999b64f9743f5b1c31026bd2d1b513971a2b68ec`. The tested implementation identity is
`4a5f263e012595e3db50d35dc9f23355572560791f58c0c56f146ddaa8a5a813` (SHA-256 over the compile directory and the
value interface, calculated by `run.py`). The native candidate was rebuilt
with the existing Whitefoot compiler; PUC `luac` was built from Redis Lua C
sources and headers copied into scratch outside the repository.

## Results

| Observation | Result |
| --- | --- |
| Lua sources compared | 602 |
| Existing Halo oracle scripts | 80, all matched |
| Generated valid programs | 445, 443 matched |
| Malformed programs | 77, all messages matched |
| Candidate/oracle cells compared | 32558, all matched |
| Oracle cells across every valid program | 33383 |
| Comparison mutation controls detected | 10 |
| Interning exhaustion checks | 6, all matched the required memory error |
| Registered module check | All five modules accepted in 1.37 seconds |

The comparison exits **1**, preserving the two unsupported-program failures.
It is not a full acceptance pass. Each successful program also compares all
constants, prototype metadata, absolute targets, capture pseudo-cells and
per-cell line information; the final 256 Nil padding entries are included.

The final direct `whitefootc --graph lib/halo/modules.wfg --check-modules`
check exited 0 in 1.37 seconds, with 1.27 seconds user and 0.08 seconds system
time. No module in this worktree took minutes on this run. The native dump
rebuild exited 0 in 2.79 seconds using the modular cache. Before the local
review repair, the preliminary three-case sample exited 0 and compared 133
cells in 10.0–665.5 ms per case. This sample's large spread prompted the full
short comparison. That first comparison built PUC in scratch: 0.186 seconds
for its single-C-file sizing sample, then 1.493 seconds for the remaining
build. After the repair, the final comparison reused a scratch PUC build and
took 4.71 seconds; its slowest case was the new child-prototype overflow
witness at 353.6 ms. These are run-sizing observations, not compiler
performance claims. The comparisons were run on the supplied macOS host;
Linux/glibc qualification and VM execution were not run.

`git diff --check` passed. The focused static guidance sample through
`run-check.pl` exited 75 before running because another worktree held the
host-wide check lock. Static checks and the full `make check` gate are
unverified; no Cargo invocation was made.

Reproduce with the commands in README.md. The full runner's SHA-256 was
`bbe0230d4ba4eafb4f830a1020e03993b80006569c51803df6260ca9b912f300`.
The implementation hash covers the compile directory and value interface;
the driver hash separately identifies fixture generation and normalization.

## Every remaining mismatch

None. The first run left two generated programs, `globals-over-256` and
`globals-after-many-constants`, refused with a compiler limitation: the cell
enum had only `GetGlobal(a: u8, k: u8)` and `SetGlobal(a: u8, k: u8)`, while
PUC indexes a global's name constant with its wider Bx field. The lead then
added `GetGlobalX(a: u8, k: u32)` and `SetGlobalX(a: u8, k: u32)` to
`pkg::value`, which the compiler emits for a name constant at index 256 or
above and the dump prints under the same names. The rerun on 2026-10-04
compared all 525 valid programs and all 33,383 expected cells equal, and all
77 malformed programs' messages equal, with the 10 mutation controls
detected.

## Scope and limits

- Only `lib/halo/compile/`, the added Script declaration in
  `lib/halo/value/module.wfm`, and this experiment directory changed.
- Script has exactly the requested four arrays. Main is prototype zero;
  prototypes use contiguous script-wide code/constant ranges and absolute
  cell targets. Constant operands remain relative to their prototype kbase.
- Every Lua 5.1 statement and expression form requested is covered, including
  repeat-scope locals and captured locals, multiple assignment conflicts,
  generic/numeric loops, self, tail calls, open returns, RK variants,
  constants above 256, and SETLIST batching and its extra data word.
- PUC legacy vararg HASARG/NEEDSARG bits are not explicit in the supplied
  boolean Proto field. Hidden `arg` locals, registers and cells match PUC,
  but the VM must preserve that legacy behavior; runtime behavior was not
  tested here. Numeric constants compare PUC listing text (`%.14g`), not
  every binary64 payload bit; the number package documents its pow limits.
- PUC allocation-growth errors such as `too many local variables` contain
  no source location. Their complete message bytes compare directly; Halo
  still returns the scanner line, which has no PUC line field to compare.
- No specification, design-tree or conformance evidence changed. No Cargo,
  network, push, pull request or main merge was performed.

## Found along the way

- Corrected the oracle mapping of TEST: PUC prints unused B as well as C.
- Corrected SETLIST counter decoding: PUC printing treats every operand
  above 255 as an RK-like negative number, including this non-RK counter.
- Added PUC’s total 32,767-local limit, independently witnessed by 32,768
  successive `do local x end` blocks; its raw growth-error text now matches.
- Matched luac’s initial C-call depth when checking assignment/syntax depth.
- Fixed the missing PUC per-parent 262,143-child-prototype limit exposed by
  the independent review. The malformed fixture `error-076` creates 262,144
  direct children and now returns raw `constant table overflow`.
- Added `method-after-many-constants` to observe LoadKx followed by SelfR.
- Kept the wide-global interface gap visible as the two failures above.
  Its reopening condition is authorization to add a wide global cell.

## Review

A separate read-only `gpt-6.1-sol` reviewer checked
`999b64f9743f5b1c31026bd2d1b513971a2b68ec` through
`6c85f46f3b50f940da372e2abe5059796b53c52f`, plus the trailing-space repair,
against checklist groups A, D, C, M and V. Groups T and R were not applicable:
no specification, gate or material design selection changed. The review read
the changed files, VM.md, applicable design guidance, recorded comparison JSON
and PUC parser/code-generator sources. It found one defect (C1/C3/DC4): the
missing per-parent child-prototype limit. No other finding was reported.

The implementer rechecked this local repair against PUC; the reviewer did
not review the repair itself. Before rebuilding, the exact witness
`b"g=function() end; " * 262144` made PUC exit 1 with `constant table overflow`
while Halo returned a Script; after rebuilding, `--filter error-076` exited 0
and compared the complete error bytes. The final full comparison includes it.
Two adjacent positive observations also passed with both compilers: exactly
262,143 direct children, and a script with two parents each containing
131,072 children. The latter distinguishes a per-parent limit from a mistaken
script-wide limit. Those positive checks used the same repeated Lua statement,
PUC without `-l`, and Halo's public dump, checking PUC acceptance and a Script
result rather than comparing their large listings. PUC took 0.251 and 0.245
seconds; Halo took 0.365 and 0.367 seconds. These observations do not certify
VM execution.

No specification rule or design-tree decision changed, and no owner decision
is open. The explicit task limits keep this delivery local and its deferred
interface findings here, rather than changing the tree, TODO or PR.
