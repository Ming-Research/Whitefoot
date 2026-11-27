# Verification

How Whitefoot itself is checked: local heavy commands, the tests and what each owns, time budgets, the hosted gate and when it runs, and the paired performance comparison.

### decision heavy-commands-share-one-host-lock
Local heavy verification runs under one host-wide lock across worktrees, with every available processor unless the caller names fewer, visible phase accounting and a command deadline.
- replaced: Each agent's heavy commands spending the whole host independently was on main; the owner changed it, because overlapping optimized builds and suites made an ordinary build take minutes with no test progress (data: `research/investigations/test-economy/build-and-test.md`).
- replaced: A fixed pool of two processes for local verification was on main; the owner changed it, because the host-wide lock already excludes the commands the pool kept room for, so it only left half of a four-core host idle (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed setting the pool from the host's online processor count; the owner rejected it, because Cargo's and the test harness's defaults already use every processor available to the process and honor a container's quota, which that count ignores.

### decision test-programs-stop-after-sixty-seconds
Every program a test compiled runs as its own process group with both outputs drained and is stopped after 60 s; the conformance adapter reports such a stop as a stopped case, never as a verdict.
- rejected: an agent proposed the command deadline alone for a program that never finishes; the owner rejected it, because it stops the whole command only after 30 minutes, before the stuck suite's summary is written (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed the test harness's report of a long test; the owner rejected it, because stable libtest only reports a test running past 60 s and never stops it.

### decision tests-reuse-immutable-build-products
Tests reuse immutable native objects and dependency-tracked example executables, rerun every assertion, and keep distinct compiler modes and macro-interposed builds apart.
- replaced: Rebuilding every support unit for each case was on main; the owner changed it, because repeated compilation of identical support code checks no new behavior.
- rejected: an agent proposed caching test verdicts, shared between the unit tests and the native conformance adapter; the owner rejected it, because the adapter must still reach each verdict through its own ordinary compilation path.

### decision local-app-cases-use-a-persistent-cache
Outside CI a program case builds an app under `apps/` with the compiler executable and a persistent incremental cache; under CI it compiles the app in-process like every other program case.
- rejected: an agent proposed one build route everywhere; the owner rejected it, because alternating the routes keeps both the shipped cached build and the in-process build under test, while local reruns get faster.
- rejected: an agent proposed the persistent cache on CI as well; the owner rejected it, because CI runners start empty, so the cache would save nothing there.

### decision verification-builds-the-gate-profile
Compiler-backed verification uses the optimized `gate` Cargo profile with debug assertions and overflow checks kept; dev builds of the compiler are only for debugging its Rust code.
- rejected: an agent proposed building and running an extra unoptimized compiler for verification; the owner rejected it, because unoptimized proof analysis makes real-source tests much slower and adds no assertion coverage over this profile.
- rejected: an agent proposed a lower optimization level for the `gate` profile; the owner rejected it, because level 1 built 19% faster but ran the cases 9 to 15% slower, and level 2 saved 1.4%, against a 20% criterion (data: `research/investigations/test-economy/time-budgets.md`).

### decision stage-budgets-follow-each-cpu-model
A Linux stage budget per CPU model with 20 or more recorded runs is 1.1 times the maximum of the latest 50 plus its distance above their median, rounded up to 5 s; an unmapped model takes the largest.
- replaced: One budget per runner class at 1.25 times the slowest recorded run was on main; the owner changed it, because Linux runners differ by up to 1.6 times by CPU model, so slow ones tripped on noise and fast ones hid growth (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed one Linux budget recomputed at the observed maximum plus 10%; the owner rejected it, because a test could slow by 50% on a fast runner without tripping it.
- rejected: an agent proposed 1.5 times the slowest recorded run; the owner rejected it, because on identical source a typical Ubuntu unit run could then grow by 57%, and the fastest of seven runs by more than double, before its stage tripped.
- rejected: an agent proposed a 20% margin instead of 10% when the table is recomputed; the owner rejected it, because it gives up catching slowdowns under 30%, while recomputing at 10% already cuts false overruns to about 1%.
- rejected: an agent proposed budgets taken from the main line's recent stage times per model; the owner rejected it, because two models lack enough main-line runs, so it cannot replace the table, and small steps of growth would be absorbed unseen.
- rejected: an agent proposed timing every push's gate against its merge base; the owner rejected it, because it doubles every job's builds and cases to detect growth that a fixed budget detects beyond a stage's headroom.

### decision a-class-budget-margin-follows-its-own-spread
A class's margin grows with its own spread above its median: the unit group's quiet classes keep +26% to +42% headroom, under the 57% once refused, and EPYC 9V74, whose runs vary most, +74%.
- replaced: A margin of 10% over the maximum was on main; the owner changed it, because the gate's first attempt on main turned red in 22 jobs of 20 runs on overruns of at most 1.108 times the budget, every test passing (data: `research/investigations/test-economy/time-budgets.md`).
- replaced: Refusing headroom past 57% (1.5 times the slowest run) and a 20% margin, as 10% cut false overruns to about 1%, was on main; the owner changed it, because 22 jobs went red at under 1.11 times, and only EPYC 9V74 passes 57% (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed a fixed 30% over the maximum; the owner rejected it, because it leaves a quiet class +40% to +50% before it trips and sees a 30% slowdown of every stage in 42 of 756 replayed jobs against 362 (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed a fixed 25% over the maximum; the owner rejected it, because it sees that slowdown in 118 of 756 replayed jobs against 362, with the same single red job with no slowdown (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed keeping 10% and rerunning an overrun stage once; the owner rejected it, because a stale table fails reruns too (6 of 18 red again) and a real slowdown goes red a gate later (data: `research/investigations/test-economy/time-budgets.md`).

### decision new-budget-rows-are-set-from-the-runs-they-have
A budget row new to the table sets each Linux class once by the budget rule from the runs it has, a class with no run taking the row's largest, and keeps it until 20 runs.
- rejected: an agent proposed setting a new row at 1.5 times its longest run, as the proof and sanitizer rows are; the owner rejected it, because it reports growth only past half, wider than the budget rule's 26% to 74% headroom (decision a-class-budget-margin-follows-its-own-spread).
- rejected: an agent proposed sampling a new row until each class has 20 runs before writing it; the owner rejected it, because that needs about 80 more cold builds, 3 hours of shared runner time, for a stage that never fails a job (data: `research/investigations/test-economy/time-budgets.md`).

### decision owner-added-columns-take-the-larger-value
A Linux column the owner adds for a model with fewer than 20 runs of a stage takes the larger of the row's largest recomputed class budget and the model's own runs by the budget rule.
- rejected: an agent proposed that the recompute script leave such columns untouched for hand edits; the owner rejected it, because those cells would fall behind as other models rise, bringing back false overruns unless someone remembered to edit them.
- rejected: an agent proposed dropping the owner-added column and using the unmapped-model fallback; the owner rejected it, because it undoes the column the owner chose, and the fallback ignores the model's own runs where they need more.
- rejected: an agent proposed setting the Xeon 8573C column of the candidate-compiler build row from that model's own few runs alone; the owner rejected it, because the row's largest value must not be lowered by a model with few runs, as for every other owner-added column.

### decision candidate-compiler-build-budget-is-set-per-processor
The candidate compiler's build budget follows the table's rule per processor class over cold builds after the toolchain warm-up: 160 s on the EPYC 7763, 185 s on the 9V74, 140 s on the 9V45.
- replaced: One budget for every Linux class (145 s, 160 s since the tenth added to every Linux budget) was on main; the owner changed it, because the 145 s had been exceeded four times, up to 152.0 s, and the classes differ in speed (data: `research/investigations/test-economy/time-budgets.md`).
- replaced: A budget the owner set per class (165, 150, 135 and 165 s) was on main; the owner changed it, because it was set from builds whose first took 7% longer than a second, which loosened it (data: `research/investigations/test-economy/time-budgets.md`).

### decision the-xeon-6975p-has-a-budget-column
The budget table has a column for the Xeon 6975P-C, which took 56% of the sampled hosted jobs; it is an owner column whose cells started at each row's largest Linux value.
- rejected: an agent proposed no column for it, leaving it on each row's largest class value; the owner rejected it, because the candidate-build cell would then not tighten for most runs (data: `research/investigations/test-economy/time-budgets.md`).

### decision overruns-fail-only-the-final-verdict
Hosted CI records a budget overrun without changing the stage's status and fails its final verdict for Linux and Windows overruns or a missing budget row; macOS overruns are only reported.
- replaced: Failing the verdict on a macOS overrun was on main; the owner changed it, because one macOS runner model varied 1.8 times between runs, so any fixed budget fails often or misses real slowdowns, which Linux catches (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed failing a stage's own command when it exceeds its budget; the owner rejected it, because a step that reads its command's status would mistake or hide the overrun, and the first overrun would stop the job's later stages.
- rejected: an agent proposed job timeouts sized to the budgets as the only time bound; the owner rejected it, because they stop a job mid-stage and throw away its results and its ranking of slowest cases.
- rejected: an agent proposed re-running a job automatically when only a budget turned it red; the owner rejected it, because a real slowdown would then need two overruns to show, and a stale budget trips on both attempts.
- rejected: an agent proposed making Linux overruns report-only too; the owner rejected it, because then no slowdown would ever turn the gate red, and Linux is where slowdowns are caught.

### decision overruns-are-judged-from-the-change
An overrun is judged from the change: a cause found is fixed or its raise goes to the owner, an unclear one is re-run once, and one the change plainly cannot explain is reported as runner variance.
- rejected: an agent proposed treating every overrun as a cause to fix or a raise for the owner; the owner rejected it, because clippy's threefold spread on identical source would bring the owner runner noise that the change itself shows to be noise.
- rejected: an agent proposed shortening repeated schedules or removing normative cases because they are slow; the owner rejected it, because that removes the behavior evidence the gate is meant to keep.

### decision windows-raises-add-ten-seconds
A Windows budget raise the owner approves adds 10 s to the stage over its budget and to the group stage that contains it, if any.
- replaced: Recomputing one and a quarter times the slowest run at each raise was on main; the owner changed it, because a fixed step keeps every raise small and alike, while the recomputed margin grows with every slower run it absorbs.

### decision gate-builds-are-incremental-only-locally
Builds of the `gate` profile compile incrementally, except on GitHub's hosted runners, where CI turns incremental compilation off.
- rejected: an agent proposed one construction for local and hosted builds; the owner rejected it, because an edit took 83 s and 141 s to rebuild without it and 10 to 25 s with it, while a fresh CI checkout ran 3 to 9% slower with it (data: `research/investigations/compiler-architecture/DESIGN.md`).

### decision one-make-inventory-drives-local-and-ci
Local correctness checks and the hosted correctness jobs use one Makefile group inventory and its recipes; CI derives its matrix from it and keeps only host setup and scheduling.
- replaced: A CI command list written apart from the local one was on main; the owner changed it, because separately written workflow commands can drop a local check or add an unreported extra even when both call Make targets (data: `research/investigations/test-economy/redesign.md`).

### decision a-green-hosted-gate-is-make-check
A green hosted gate on the exact revision to be merged, a head current with main, is that revision's `make check`; a local run reproduces a failure or stands in when CI is down.
- rejected: an agent proposed that every author run the full gate locally before handing work back; the owner rejected it, because the hosted jobs run the same Makefile groups and recipes, while a local run repeats them on one host at the author's cost.

### decision branch-checks-share-one-merge-base
The checks that compare a branch with main read the merge base of origin/main and HEAD, which one script fetches first unless GITHUB_ACTIONS=true, then reads as checked out, and names in each run.
- replaced: Taking the specification archives' and the README translation's merge base with the local branch main was on main; the owner changed it, because a stale local main charged a branch that had merged newer origin/main with archives and log entries it never added.
- replaced: Comparing the specification's readiness with origin/main's latest commit was on main; the owner changed it, because main's later changes then read as the branch's own and asked it for log entries it did not owe.
- rejected: an agent proposed taking the local origin/main as it stands and naming the base in the output; the owner rejected it, because a stale origin/main puts the base before the fork point once the branch has merged newer main, so the same false reports stay and the base can differ from CI's.

### decision research-stays-out-of-daily-ci
The correctness gate and the paired performance checks depend on no research program, script, fixture or dataset; what they need is moved into formal tests, and research runs only when asked.
- replaced: Experimental models, repeated measurements and shared research inputs wired into ordinary verification was on main; the owner changed it, because they had no permanent-test grounds of their own (data: `research/investigations/test-economy/redesign.md`).
- rejected: an agent proposed relying only on manual review, or withholding research inputs from CI, to keep the boundary; the owner rejected it, because light reference checks catch accidental dependencies with normal complete checkouts.

### decision research-tool-self-tests-run-in-their-manual-workflow
A research tool's self-tests run as a step of the manual workflow that uses the tool, after the step that extracts what they read, and no gate group runs them.
- rejected: an agent proposed a gate group that extracts the prototype commit and runs the self-tests each check; the owner rejected it, because the gate stays free of research programs, and the group would tie it to one commit's reachability and add a stage for tools only a manual workflow uses.

### decision manual-workflows-reach-their-prototype-through-a-retention-tag
A prototype commit that a manual workflow extracts from and tree links name is kept by a retention tag outside the `wf-` prefix; the workflow reaches it only through that tag and checks the commit id.
- rejected: an agent proposed restoring the prototype's research directory into main so the workflow reads it from the repository; the owner rejected it, because it adds about 40 files and their tests as a copy that only convention keeps unedited, and the tree links naming the commit still need it.

### decision assertions-live-with-their-contract
A verification assertion lives with the contract it checks: specification requirements in conformance, whole-program behavior in programs, other compiler obligations in compiler tests.
- rejected: an agent proposed assigning WF wrappers to Rust unit tests by fixture language; the owner rejected it, because a useful property is placed by what it checks, apart from its fixture language and executable packaging (data: `research/investigations/test-economy/redesign.md`).

### decision new-compile-tests-name-their-gap
A new test that compiles WF or runs its output names the missing coverage and the extra build or run it needs, and prefers an existing case or a more direct assertion with the same evidence.
- rejected: an agent proposed treating every WF fragment as grounds for another compile-and-run case; the owner rejected it, because repeated end-to-end construction adds up across the suite, and future compiler speedups do not justify redundant cases now.

### decision paired-timing-builds-each-revisions-workload
Paired performance compiles each revision's own formal version of the same workload with that revision's compiler, after checking that workload names, fixtures and oracle interfaces match.
- rejected: an agent proposed compiling the current sources with both revisions' compilers; the owner rejected it, because a syntax or ABI change can leave the merge-base compiler unable to build an equivalent workload, giving no comparison at all.
- rejected: an agent proposed silently skipping a mismatched or unbuildable workload; the owner rejected it, because the absence of comparable evidence is not a passing regression result.
- rejected: an agent proposed running the code-placement control on every measured comparison; the owner rejected it, because that adds a campaign to each run, while the emitter's own test already fails on a definition without the alignment (data: `research/investigations/code-placement/DESIGN.md`).

### decision paired-timing-skips-byte-identical-images
Both arms of the paired comparison build their runtime, runner and oracle with one source-path prefix map, and a program whose two linked images are byte-identical is not timed and passes.
- replaced: Timing all five programs behind one host control was on main; the owner changed it, because the arms' images differed in every retained run through the embedded checkout path, so unchanged programs were timed and failed (data: `research/investigations/identical-code-noise/README.md`).

### decision an-undecided-program-is-retested-once
A timed program is judged against its own null control; a failing comparison with an adverse null is measured once more alone, and one still undecided is inconclusive and passes the job.
- replaced: Ending the whole comparison as inconclusive when any host control failed, with no retry, was on main; the owner changed it, because the control reads at the threshold in 2.3% of cells (data: `research/investigations/identical-code-noise/README.md`).
- rejected: the owner proposed judging an undecided program on the 14900K machine; the owner rejected it, because only the same-host retest can be checked against the 182 retained runs, and the other adds a self-hosted job and queueing.

### decision kernels-placed-by-their-data-are-not-judged
A kernel whose arms map differently and whose 32 positions in order of time differ in two rounds is neither timed nor judged; the job passes with a warning and judges the rest.
- rejected: an agent proposed timing every kernel's arms at the 32 positions in each run, with no mapping record; the owner rejected it, because that adds about two minutes on AMD hosts, two to three campaigns, to every run (data: `research/investigations/data-placement/README.md`).
- rejected: an agent proposed setting a kernel aside when the candidate alone is 10 percent off at one position twice; the owner rejected it, because every image has one on Zen 3 and Zen 4 hosts, so `stencil` would never be judged there (data: `research/investigations/data-placement/README.md`).
- rejected: an agent proposed failing the job as inconclusive when a kernel is set aside; the owner rejected it, because a change of startup would then turn red on every Zen 3 or Zen 4 host and pass only when a rerun drew another host.

### decision performance-comparison-warms-the-toolchain-first
The paired performance comparison fetches the locked crates and reads the Rust toolchain in one untimed step before its first compiler build, outside every labeled stage.
- rejected: an agent proposed building the candidate compiler once untimed first, as the kernel images are; the owner rejected it, because it removes any first-use cost but adds a cold build of 89 to 147 s per run (data: `research/investigations/test-economy/candidate-build-samples-2026-11.tsv`).

### decision conformance-cases-are-only-runnable
Conformance has only the `runnable` manifest status and pass or fail outcomes; an unexpected unsupported stop fails the case.
- rejected: an agent proposed pending or xfail statuses, or a third outcome for a gap that does not fail; the owner rejected it, because a capability gap is a compiler defect, not grounds to suspend a normative case.

### decision conformance-verdicts-come-from-the-front-end
Accept and reject conformance cases take their verdict from the same front-end continuation that full compilation uses; run cases go on through lowering and target compilation.
- rejected: an agent proposed requiring every source-only verdict to produce a native image; the owner rejected it, because STOR-6 begins after source acceptance, and an incidental host address domain cannot replace the source verdict.

### decision conformance-rejections-pin-their-span
A single-source conformance rejection may assert a half-open byte span, and the ordinary and cached adapters compare its source, span and cited rule from the same rejection.
- replaced: Comparing only the cited rule was on main; the owner changed it, because a rule match alone missed a report marked at a type argument where PROV-6 selects the whole call [DIAG-1].

### decision gate-linux-uses-one-pinned-llvm
The gate's Linux jobs on GitHub's hosted x86-64 runners use one LLVM major from apt.llvm.org, pinned in `.github/llvm-major` for the gate and the releases; raising the pin is its own change.
- replaced: Ubuntu's default clang was on main; the owner changed it, because the owner wants a recent clang held at one version, and under Ubuntu 24.04's clang 18 no gate job reached the `preserve_none` dispatch path (data: `research/investigations/match-dispatch/DESIGN.md`).
- rejected: an agent proposed a floating latest LLVM version; the owner rejected it, because a new LLVM release would change the probed forms and generated code under a revision that had passed.

### decision gate-runs-where-a-merge-is-decided
The gate and the I/O host checks run for a push to main, a push to a branch with an open ready pull request, a ready pull request's opening or reopening, the ready event and a manual run.
- replaced: Running the gate for every push was on main; the owner changed it, because about a third of the gate runs for branch pushes were for a branch with no open pull request, whose results nobody uses (data: `research/investigations/test-economy/time-budgets.md`).
- replaced: Running the gate for every push of a draft pull request was on main; the owner changed it, because pushes to a draft check revisions the author is still changing, up to 10 runs on one branch (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed skipping branches by name, such as `research/`; the owner rejected it, because research and code work share `claude/` branch names, and a research branch with an open pull request still needs its gate.
- rejected: an agent proposed triggering on pull request events alone; the owner rejected it, because GitHub sends no pull request event while it conflicts with its base, so pushes to a conflicting branch would go unchecked.

### decision io-hosts-and-sanitizers-check-what-a-change-touches
The I/O host checks and the sanitizers check a revision on main whole, main's head again each night, and any other revision only where its change from main touches a path their jobs read.
- replaced: Checking every revision the gate checks, whatever its change touched, was on main; the owner changed it, because in 38 merge checks the sanitizers never failed, each check spent 6 to 7 minutes on each of the two workflows, and two of three merges changed no compiler code.

### decision pull-request-openings-run-in-their-own-group
A pull request's opening or reopening runs in a concurrency group of its own; an opening skips if the head has moved by the time it decides, and a ready pull request's reopening always runs.
- rejected: an agent proposed letting the opening's run share the push group; the owner rejected it, because it could cancel the run of a newer head pushed right after the opening and leave that head unchecked.
- rejected: an agent proposed checking the opening's head even after a newer push replaced it; the owner rejected it, because the newer head is checked by its own push run.

### decision final-gate-job-skips-cancelled-runs
The final job "Gate on both platforms" runs on `!cancelled()`, so a run that a later push supersedes leaves no result.
- replaced: Running the final job on `always()` was on main; the owner changed it, because the red result of a superseded run would sit beside the run that replaced it and be misread.

### decision failed-scope-query-fails-the-run
A failed GitHub query in the step that decides whether to check fails the step and the run.
- rejected: an agent proposed skipping the checks when the query fails; the owner rejected it, because a silent skip would leave a mergeable revision unchecked.

### decision macos-joins-only-hand-started-gates
Every gate run that checks a revision runs every group on Linux; a manual run started with its `macos` input runs them on macOS too, and a release needs such a run on its commit.
- replaced: Running every group on both platforms in every push was on main; the owner changed it, because macOS jobs waited far longer than Linux jobs for hosted runners, and nothing could make sessions push less (data: `research/investigations/test-economy/time-budgets.md`).
- replaced: Running both platforms in every run that checks a revision was on main; the owner changed it, because the hosted macOS runners take five jobs at a time, and a run's macOS jobs waited about 43 minutes while each ran under 10 minutes, holding merges back.
- rejected: an agent proposed running macOS on each push to the main line; the owner rejected it, because the queue moves behind the merge, every main commit spends four macOS jobs though few are released, and a macOS-only fault turns the main line red.
- rejected: an agent proposed running one macOS group for each ready pull request; the owner rejected it, because each ready pull request still takes a macOS runner, so ten at once would still queue.
- rejected: an agent proposed releasing from a gate run without macOS jobs; the owner rejected it, because the released macOS compiler would have run only one smoke program on macOS and none of the gate's groups.
- rejected: an agent proposed a release workflow that runs the gate's groups on macOS itself; the owner rejected it, because the result would sit in the release run, not a gate run of the commit, so the release lookup and the final job would read different records.
- rejected: an agent proposed a daily scheduled macOS run of the main line as well; the owner rejected it, because the scheduled commit is seldom the one a downstream needs, so it cannot replace the release run, and its failures need someone to watch them.

### decision slowest-gate-groups-run-in-parts-on-linux
On Linux the gate runs the unit group as four parts and the corpus group as five, each building for itself; macOS and local runs run groups whole, and the final job requires every matrix job.
- replaced: Running each group whole on Linux was on main; the owner changed it, because the corpus group took 438 s and the unit group 402 s in a recorded gate run, so the slowest group kept the push-to-result time over six minutes (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed building once and sharing the test binaries as an artifact that the part jobs download; the owner rejected it, because the build job, the upload and the download would all lie on the critical path, and the build is most of a part's time.
- rejected: an agent proposed caching the Cargo dependencies, the build directory and LLVM between pushes; the owner rejected it, because a cold run took 228 s and a warm one 225 s, which saves nothing (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed cargo-nextest to divide the unit tests; the owner rejected it, because it adds a tool and runs each test in a process of its own, which changes how the existing tests share process state.
- rejected: an agent proposed hashing test names into parts; the owner rejected it, because hashing can leave uneven test counts, while round robin over sorted names balances them.

### decision final-gate-job-retries-github-queries
When its own run's groups did not all succeed, the final job "Gate on both platforms" retries a failed GitHub query five times and lists again up to four times while an expected job is missing.
- replaced: A single listing of the revision's runs and jobs was on main; the owner changed it, because on one pull request with every group green it failed four times in a row, twice on HTTP 502 or 504 and twice on listings that lacked passed jobs.
- replaced: Counting a cancelled job as its result was on main; the owner changed it, because a cancelled job belongs to a superseded or stopped run, and a duplicate run cancelled one second after its twin finished a job let that cancellation decide it.
- rejected: an agent proposed listing again after any conclusion other than success too; the owner rejected it, because a failed job is already final for its run, and waiting for it to change only delays a red verdict.

### decision final-gate-job-takes-its-own-runs-success
The final job "Gate on both platforms" takes the result of its own run's groups as the verdict when it is success, with no GitHub call, and reads the revision's runs and jobs only when it is not.
- replaced: Judging every run by the revision's job listing was on main; the owner changed it, because GitHub recorded finished group jobs as complete minutes late, so a passed group read as absent (data: `research/investigations/test-economy/time-budgets.md`).
- rejected: an agent proposed keeping the listing and counting a group it lacks as the run's success; the owner rejected it, because the final job would still need GitHub's answer, so repeated HTTP 502 would still turn a green run red, and a failure already listed would still decide.
- rejected: an agent proposed dropping the listing and judging only by the run's own groups, deleting the summary script, its self-test and the Makefile line; the owner rejected it, because the listing still decides where a later success of another run settles a run whose groups did not succeed.

### decision ready-event-reuses-a-manual-run
Marking a pull request ready skips the gate and the I/O host checks when its head already has a successful manual run of the same workflow, and runs them when it has none.
- rejected: an agent proposed running the checks at every ready event; the owner rejected it, because a head the author already checked by hand would be checked again on both platforms, about 6 to 7 minutes each, for the same result.
- rejected: an agent proposed checking only by hand, with no ready trigger; the owner rejected it, because a pull request marked ready without a manual run would reach the merge unchecked, and a merge made around the merge check would put a broken revision on main.

### decision performance-comparison-waits-for-ready
The paired performance comparison runs for a ready pull request, its later pushes and by hand, and shows as skipped for a draft.
- replaced: Running the comparison on every pull request push was on main; the owner changed it, because it times two compilers and may first build the baseline compiler, for up to 20 minutes, which every push of a draft would repeat.

### decision emitted-llvm-is-verified-explicitly
The backend tests of indexed marks and fields run the host clang's LLVM verifier on their emitted modules through one shared helper, not only the link.
- rejected: an agent proposed dropping the explicit verifier calls and relying on clang rejecting invalid IR when it builds the executable; the owner rejected it, because that coverage depends on how the host clang was built and its message is cut from the CI result.
