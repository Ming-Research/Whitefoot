# Merge-base compiler cache

How the paired performance comparison gets its baseline compiler, the one built from the pull request's merge base, and how that build's time is judged.

### decision the-baseline-compiler-is-cached-per-main-commit
The merge-base compiler is cached under an exact key of the main commit and the toolchain, saved by each push to main, so a pull request on that commit restores it instead of building it.
- replaced: A cold build of the merge-base compiler in every pull request run was on main; the owner changed it, because every pull request on one main commit built the same compiler, and it overran its budget on about one run in four (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed only raising the budgets of both compiler builds; the owner rejected it, because CI would get no faster, and the builds would reach the line again as the compiler grows.
- replaced: The merge-base build's target directory, restored from the shared Cargo cache by key prefix, was on main; the owner changed it, because the key must name the main commit and the toolchain, or a run could take another commit's build as its baseline.

### decision a-cold-baseline-build-only-reports-its-time
A cold build of the merge-base compiler is compared with its budget row and reported, but its overrun never fails the verdict; the candidate compiler's build stays enforced.
- replaced: An enforced budget on the merge-base build was on main; the owner changed it, because that build is the main line's own code, which no pull request changes, so failing it only turns unrelated pull requests red, while a slower main also slows the candidate build, whose budget catches it.
- rejected: an agent proposed keeping the merge-base budget enforced, recomputed per processor model at up to 155 s; the owner rejected it, because unrelated pull requests would still turn red, and main could slow by over 30% before the budget caught it.
- rejected: an agent proposed raising the merge-base budget to 135 s; the owner rejected it, because the number would have no use once the build only reports, and a cold build on a slow runner could still pass it.