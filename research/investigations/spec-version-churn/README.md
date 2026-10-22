# Specification version churn

Question: why do open amendments of the specification chase one another each
time one of them merges, and which design removes the chase?

Until this investigation an amendment took the next version on its branch: it
retitled `spec/kernel-spec.md`, archived the outgoing version under
`spec/kernel-spec-vN.md`, and headed its `spec/log.md` entry with the version.
Every amendment that merged first left every other open one in conflict on all
three. The design chosen is recorded in
[build-inputs](../../../design/compiler/build-inputs.md); the specification
no longer carries a version, an amendment adds no archive, and an entry's
version is derived from its place in `spec/log.md`
(`.github/spec-version.sh`).

## Measurement

Population: the 62 pull requests that changed `spec/kernel-spec.md` and merged
into `main` from 2026-10-07 to 2026-10-21, taking the specification from v0.99
to v0.160. For each, the branch is `M^1..M^2` of its first-parent merge commit
`M`. A merge of main into a branch is a merge commit on the branch whose second
parent is an ancestor of `M^1`. A collision is such a merge where both sides
changed the title line since their merge base, or both changed `spec/log.md`
(an upper bound for the log: two changes that do not touch adjacent lines merge
without a conflict, though two entries added at the top always touch).

```sh
for M in $(git log origin/main --first-parent --since=2026-10-07 --format=%H -- spec/kernel-spec.md); do
  base=$(git merge-base "$M^1" "$M^2")
  for C in $(git rev-list --merges "$base..$M^2"); do
    A=$(git rev-parse "$C^1"); B=$(git rev-parse "$C^2")
    git merge-base --is-ancestor "$B" "$M^1" || continue
    mb=$(git merge-base "$A" "$B")
    # title: the first line differs from mb on both sides, and between them
    # log: spec/log.md differs from mb on both sides
  done
done
```

Results, measured on the repository history:

| Quantity | Value |
|---|---|
| Amendments merged | 62 |
| Merges of main into their branches | 307 |
| Of these, both sides changed the title line | 48 |
| Of these, both sides changed `spec/log.md` | 88 |
| Of these, either | 112 |
| Amendments with at least one collision | 50 (81%) |
| Branch commits, all | 986 |
| Branch commits only renumbering, re-archiving or repairing the log (matched on the commit message, approximate) | 111 |

Collisions grow with the time a branch stays open:

| Open (first branch commit to merge) | Amendments | Collisions | Per amendment |
|---|---|---|---|
| under 6 hours | 16 | 14 | 0.9 |
| 6 to 24 hours | 28 | 34 | 1.2 |
| 1 to 3 days | 10 | 26 | 2.6 |
| over 3 days | 8 | 38 | 4.8 |

The 18 amendments open more than a day carry 64 of the 112 collisions. About
4.4 amendments merged a day, so the expected number of collisions of one
branch grows with both its open time and that rate (deduced).

Examples: the branch of Whitefoot#470 renumbered at 15:05, 15:19 and 15:55 UTC
on 2026-10-21 (v0.158, v0.159, v0.160) because #510, #462 and #470 merged at
15:17, 15:52 and 16:09; Whitefoot#484 merged main ten times in 33 hours,
three of the merge commits titled as renumbering.

Other findings:

- The design tree log `design/log.md` collides the same way: 130 pull requests
  changed it in the period, 106 of them with both sides changed, 343 times. It
  already merges with git's `union` driver, which keeps both blocks but drops
  the blank line between them (reproduced in a scratch repository), so
  branches still carry repair commits. Its form belongs to the shared design
  tree rules and is outside this change.
- Nothing read the version: `SPEC_VERSION` and its alias
  `ACTIVE_KERNEL_SPEC_VERSION` had no user, and the compiler's identity is the
  SHA-256 of the bytes. The release workflow copied the title into the release
  manifest; it now takes the version from the log.
- Each archive is a copy of the 0.7 MB specification: 160 archives are 74 MB
  on disk (`du`), and every amendment added one.
- The date in a log heading is the day the entry was written. Of the first 63
  headings, 14 share 2026-10-13 and 4 stand out of date order against the
  log's order, which is the merge order, so a date names an amendment only
  together with its title and says nothing about order.

The cost of the collisions is not measured: the board's dispatch records start
on 2026-10-20 and are kept per session, not per cause. A deduced estimate is
one short engineer step plus one quick check run each, 1 to 5 USD, so 110 to
560 USD for 112 collisions against about 1,250 USD recorded for the two days
from 2026-10-20. The larger loss is calendar time: each chase restarts the
branch's checks.

## Alternatives

- Keep the number on the branch and renumber by script: removes the manual
  repairs only; each collision still needs a merge of main and a new run.
- Assign the number after the merge with a commit to main: changes the
  specification's bytes after the gate passed on them, needs a workflow that
  pushes to main, and edits the log heading where the next branch inserts.
- Write a date or time into the title line, or name archives by one: every
  branch still edits the title line, and an archive of the outgoing version
  needs the previous version, known only at merge.
- Name an amendment by the date and title of its entry, with no number: also
  removes the collisions; refused because the name is long, carries no order
  and does not continue the 160 existing names.
- One amendment in flight at a time: no collisions, but at the median 12 hours
  open that is two merges a day against the 4.4 observed.
- Derive the version from the log (chosen): a branch writes only its entry at
  the top of the log, which `union` keeps beside any other; the title line and
  the archives are no longer written by amendments.

## What would reject the design

Counted the same way over a later period: an amendment that still needs a
manual change to a title, an archive or a log heading after a merge of main, or
`check-spec-archives.sh` rejecting a log that `union` merged correctly, or a
version that changes for an existing entry. The check's self-test merges two
branches that each add an entry and requires the merge to be clean and
accepted.
