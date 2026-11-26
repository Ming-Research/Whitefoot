#!/bin/sh
# Adversarial checks of .github/merge-base.sh and of how the Makefile and the
# workflows use it, apart from the script's own `--self-test`.
#
# Reference: the work's acceptance conditions and its background. A branch is
# charged with its own changes only, which is what the merge base of the
# remote main and HEAD gives (what a merge would change in main), and the same
# commit gets the same base in every clone, in a linked worktree, and in a CI
# checkout. Each expectation below is stated by that, not by the script's output.
#
# Fixture: an upstream repository whose main carries a specification, its
# archive, its log (merged with git's union driver, as the repository's
# .gitattributes sets) and both READMEs; a bare remote; clones of it.
set -eu
LC_ALL=C
export LC_ALL
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
script=$here/merge-base.sh
spec_check=$here/check-spec-archives.sh
readme_check=$here/check-readme-translation.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/test-merge-base.XXXXXX")
trap 'rm -rf "$work"' EXIT
remote=$work/remote.git
passed=0

die() {
    echo "test-merge-base: $*" >&2
    exit 1
}
test -f "$script" || die "$script does not exist"
identity() {
    git config user.email test@example.invalid
    git config user.name test
}
add_entry() { # add_entry HEADING
    { awk '/^## / { exit } { print }' spec/log.md
      printf '## %s\n\nRules: X-2\n\nSummary: Amend.\n\n' "$1"
      awk 'found || /^## / { found = 1; print }' spec/log.md; } > spec/log.md.new
    mv spec/log.md.new spec/log.md
}
amend_spec() { printf 'amended by %s\n' "$1" >> spec/kernel-spec.md; }
local_base() { env -u GITHUB_ACTIONS sh "$script"; }
ci_base() { env GITHUB_ACTIONS=true sh "$script"; }
# The checks the Makefile runs with the base the script chooses; $1 is how the
# script is run (local_base or ci_base).
chain() {
    base=$("$1" 2>"$work/chain-err") || { cat "$work/chain-err"; return 1; }
    sh "$spec_check" "$base" || return 1
    sh "$readme_check" "$base" || return 1
    sh "$spec_check" --require-approval "$base" || return 1
}
expect() { # expect pass|reject PATTERN DESCRIPTION COMMAND...
    want=$1; pattern=$2; what=$3
    shift 3
    outcome=pass
    "$@" > "$work/out" 2>&1 || outcome=reject
    if test "$outcome" != "$want" || ! grep -q -- "$pattern" "$work/out"; then
        echo "test-merge-base: $what: expected $want matching '$pattern', got $outcome:" >&2
        cat "$work/out" >&2
        exit 1
    fi
    passed=$((passed + 1))
}
same() { # same EXPECTED GOT DESCRIPTION
    test "$1" = "$2" || die "$3: expected $1, got $2"
    passed=$((passed + 1))
}
branch() { # a clean branch of the dev clone
    cd "$work/dev"
    git reset -q --hard
    git clean -q -f -d
    git checkout -q -B "$1" "$2"
}
# main gains a specification amendment with its log entry, a new released
# archive, and a change to both READMEs, as another branch's merge would add.
upstream_advances() { # upstream_advances N
    (
        cd "$work/upstream"
        amend_spec "upstream $1"
        add_entry "2026-03-0$1 Upstream $1"
        cp spec/kernel-spec.md "spec/kernel-spec-v9.$1.md"
        printf 'upstream %s\n' "$1" >> README.md
        printf 'upstream %s\n' "$1" >> README.zh-CN.md
        git add -A
        git commit -q -m "upstream $1"
        git push -q origin main
    )
}

git init -q -b main "$work/upstream"
cd "$work/upstream"
identity
mkdir spec
printf 'spec/log.md merge=union\n' > .gitattributes
printf '# Kernel Specification\nrules 1\nrules 2\n' > spec/kernel-spec.md
printf '# Kernel Specification v0.0\nzero\n' > spec/kernel-spec-v0.0.md
printf '# Specification change log\n\nPreface.\n\n## 2026-01-01 v0.1: Base\n\nRules: X-1\n\nSummary: Base.\n' > spec/log.md
printf 'english\n' > README.md
printf 'chinese\n' > README.zh-CN.md
printf 'other\n' > other.md
git add -A
git commit -q -m base
git init -q --bare -b main "$remote"
git remote add origin "$remote"
git push -q origin main
git clone -q "$remote" "$work/dev"
cd "$work/dev"
identity
m0=$(git rev-parse origin/main)

# 1. Output contract (acceptance 1): stdout is the commit and nothing else,
# stderr one line naming that commit, the remote tip and HEAD.
branch contract origin/main
printf 'edit\n' >> other.md; git add -A; git commit -q -m other
upstream_advances 1
cd "$work/dev"
git fetch -q origin
git merge -q --no-edit origin/main
tip=$(git rev-parse origin/main)
head=$(git rev-parse HEAD)
out=$(local_base 2>"$work/err")
same "$tip" "$out" 'stdout is exactly the base commit'
same 1 "$(wc -l < "$work/err" | tr -d ' ')" 'stderr is one line'
want="review base: $(git rev-parse --short "$tip") (merge base of origin/main $(git rev-parse --short "$tip") and HEAD $(git rev-parse --short "$head"))"
same "$want" "$(cat "$work/err")" 'stderr names the base, the remote tip and HEAD'

# 2. Upstream amendments, new archives and README changes after the fork are
# not the branch's (acceptance 3), whether the branch merged them or not, and
# whatever the local main and the clone's origin/main say.
branch feature-merged "$m0"
printf 'edit\n' >> other.md; git add -A; git commit -q -m other
git fetch -q origin
git merge -q --no-edit origin/main
git update-ref refs/remotes/origin/main "$m0"
expect pass 'unchanged since' 'upstream archive and entry, merged by the branch, stale refs' chain local_base
git push -q origin feature-merged
branch feature-unmerged "$m0"
printf 'edit\n' >> other.md; git add -A; git commit -q -m other
git update-ref refs/remotes/origin/main "$m0"
expect pass 'unchanged since' 'upstream archive and entry, not merged, stale refs' chain local_base
same "$m0" "$(local_base 2>/dev/null)" 'an unmerged branch is based on its fork point'

# 3. A branch that merged an upstream amendment and amends the specification
# itself is charged with exactly its own entry (acceptance 5 with 2).
upstream_advances 2
cd "$work/dev"
git fetch -q origin
branch feature-own origin/main
amend_spec own
expect reject 'gained 0' 'an own amendment without its entry after the merge of upstream entries' chain local_base
add_entry '2026-03-10 Own amendment'
expect pass 'is new and complete' 'an own amendment with its entry after the merge of upstream entries' chain local_base
cp spec/kernel-spec.md spec/kernel-spec-v0.2.md
expect reject 'adds no archive' 'an own amendment that adds an archive' chain local_base
rm spec/kernel-spec-v0.2.md
printf 'edit\n' >> README.md
expect reject 'did not' 'an own README change without its translation' chain local_base
git checkout -q -- README.md
# A released archive that upstream added is just as unchangeable as an old one.
printf 'edited\n' >> spec/kernel-spec-v9.2.md
expect reject 'released specifications changed' 'an edit to an archive upstream added' chain local_base
git checkout -q -- spec/kernel-spec-v9.2.md

# 4. A CI checkout of a pull request is a detached merge commit of the branch
# into main's tip, with no fetch; the base is main's tip at the merge, and a
# main that has advanced since is not charged to the branch.
git fetch -q origin
branch pr-branch origin/main
amend_spec pr
add_entry '2026-03-11 PR amendment'
git add -A; git commit -q -m pr
pr_tip=$(git rev-parse HEAD)
main_at_merge=$(git rev-parse origin/main)
git checkout -q --detach origin/main
git merge -q --no-ff --no-edit "$pr_tip"
merge_commit=$(git rev-parse HEAD)
upstream_advances 3
cd "$work/dev"
git fetch -q origin
same "$main_at_merge" "$(ci_base 2>/dev/null)" 'a pull request merge commit is based on main at the merge'
expect pass 'is new and complete' 'a pull request merge commit with its entry, main advanced since' chain ci_base
# The same merge without the entry is rejected.
branch pr-bare origin/main
amend_spec bare
git add -A; git commit -q -m bare
git checkout -q --detach origin/main
git merge -q --no-ff --no-edit pr-bare
expect reject 'gained 0' 'a pull request merge commit amending without an entry' chain ci_base

# 5. A push to main: HEAD is main's tip, the base is HEAD, the diff is empty.
git checkout -q --detach origin/main
same "$(git rev-parse HEAD)" "$(ci_base 2>/dev/null)" 'on main the base is HEAD'
expect pass 'unchanged since' 'a checkout of main itself' chain ci_base

# 6. A failed fetch leaves stdout empty, the exit status nonzero, and
# origin/main where it was: a caller that reads stdout never gets a stale base.
branch feature-e origin/main
before=$(git rev-parse origin/main)
git remote set-url origin "$work/missing.git"
status=0
env -u GITHUB_ACTIONS sh "$script" >"$work/stdout" 2>"$work/stderr" || status=$?
test "$status" -ne 0 || die 'an unreachable origin outside CI exits zero'
test ! -s "$work/stdout" || die 'an unreachable origin outside CI prints a base'
grep -q 'cannot fetch main' "$work/stderr" || die 'an unreachable origin outside CI says nothing of the fetch'
same "$before" "$(git rev-parse origin/main)" 'a failed fetch moves origin/main'
# The remote loses main: still no base, still no stale one.
git remote set-url origin "$remote"
git -C "$remote" update-ref -d refs/heads/main
status=0
env -u GITHUB_ACTIONS sh "$script" >"$work/stdout" 2>/dev/null || status=$?
test "$status" -ne 0 && test ! -s "$work/stdout" || die 'a remote without main gave a base from the stale ref'
git -C "$remote" update-ref refs/heads/main "$before"
passed=$((passed + 1))

# 7. The same branch commit gets the same base from a subdirectory and from a
# linked worktree, as from the clone's root.
branch feature-w origin/main
printf 'edit\n' >> other.md; git add -A; git commit -q -m w
root_base=$(local_base 2>/dev/null)
mkdir -p sub/dir
same "$root_base" "$(cd sub/dir && local_base 2>/dev/null)" 'the base from a subdirectory'
git worktree add -q --detach "$work/linked" HEAD
same "$root_base" "$(cd "$work/linked" && local_base 2>/dev/null)" 'the base from a linked worktree'

# 8. The remote main rewritten (force push) to a commit unrelated to the old
# origin/main: the fetch follows it, and the base is the branch's merge base
# with the new main, never the old tip.
git clone -q "$remote" "$work/third"
(
    cd "$work/third"
    identity
    git checkout -q -B rewritten "$m0"
    printf 'rewrite\n' >> other.md; git add -A; git commit -q -m rewrite
    git push -q --force origin rewritten:main
)
cd "$work/dev"
old_tip=$(git rev-parse origin/main)
git fetch -q origin '+refs/heads/main:refs/heads/new-main'
new_tip=$(git rev-parse new-main)
git merge-base --is-ancestor "$old_tip" "$new_tip" && die 'the force push was meant to drop the old main'
branch feature-r new-main
printf 'again\n' >> other.md; git add -A; git commit -q -m again
git update-ref refs/remotes/origin/main "$old_tip"
same "$new_tip" "$(local_base 2>/dev/null)" 'after a force push of main'

# 9. The wiring (acceptance 9, 10, 11): nothing reads the local main, one script
# chooses the base, and the approval is never HEAD against itself.
cd "$root"
if grep -n 'refs/heads/main\|branch -f main\|check-spec-archives.sh "\?main\|check-readme-translation.sh "\?main\|?= origin/main' Makefile; then
    die 'the Makefile still reads the local main or defaults the base to origin/main'
fi
for wf in gate fast design-readiness; do
    f=.github/workflows/$wf.yml
    if grep -n 'branch -f main\|git merge-base' "$f"; then
        die "$f chooses a base itself or names main as a local ref"
    fi
    grep -q 'sh .github/merge-base.sh' "$f" || die "$f does not choose its base with .github/merge-base.sh"
done
grep -q 'sh .github/merge-base.sh' Makefile || die 'the Makefile does not choose its base with .github/merge-base.sh'
grep -q -- '--require-approval "\$\$base"' Makefile || die 'the specification approval does not take the chosen base'
if grep -n -- '--require-approval "\?HEAD' Makefile; then
    die 'the approval compares HEAD with itself'
fi
passed=$((passed + 1))

echo "test-merge-base: $passed checks pass"
