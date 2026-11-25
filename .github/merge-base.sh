#!/bin/sh
# The base the repository's checks compare a branch with: the merge base of
# origin/main and HEAD, which is what a merge would change in main. A branch
# is charged with its own changes only: not with what main gained after the
# fork, nor, once the branch has merged main, with what it merged. The local
# branch main plays no part, so how old it is changes nothing.
#
# Outside CI the script first fetches main from origin, because a stale
# origin/main puts the merge base on an older commit than the branch's real
# fork point and charges the branch with whatever it has merged since. A
# failed fetch is an error, never a fall back to the old ref, and the fetch
# never stops to ask for a password (GIT_TERMINAL_PROMPT=0). CI
# (GITHUB_ACTIONS=true) does not fetch: its checkouts use fetch-depth 0 and some
# carry no credentials. It reads the same ref, so a branch gets the same base
# in both places. The switch is the environment variable because the script is
# reached through nested make calls, where a flag would have to be threaded
# through every one.
#
# Stdout is the base commit and nothing else. Stderr names it:
#   review base: <short> (merge base of origin/main <short> and HEAD <short>)
# design/skill/review-base.sh is a different script: it picks the base of the
# design tree checks by the CI event, a main push being compared with its
# previous tip. The Makefile prefers that base for the design tree checks
# (DESIGN_REVIEW_BASE) and passes this one to every other check.
#
# `--self-test` builds a remote and clones and requires each case to be judged
# as intended.
set -eu
LC_ALL=C
export LC_ALL

usage() {
    echo 'usage: merge-base.sh | --self-test' >&2
    exit 2
}

fail() {
    printf 'review base: %s\n' "$*" >&2
    exit 1
}

resolve() {
    if [ "${GITHUB_ACTIONS:-}" != true ]; then
        GIT_TERMINAL_PROMPT=0 git fetch --quiet --no-tags origin '+refs/heads/main:refs/remotes/origin/main' ||
            fail "cannot fetch main from origin, so the base cannot be chosen; a stale origin/main would name the wrong base (CI does not fetch: it sets GITHUB_ACTIONS=true)"
    fi
    remote=$(git rev-parse --verify --quiet 'refs/remotes/origin/main^{commit}') ||
        fail "refs/remotes/origin/main does not exist; fetch main from origin (CI checks out with fetch-depth 0)"
    head=$(git rev-parse --verify --quiet 'HEAD^{commit}') ||
        fail "HEAD is not a commit"
    base=$(git merge-base "$remote" "$head") ||
        fail "origin/main and HEAD have no merge base"
    printf 'review base: %s (merge base of origin/main %s and HEAD %s)\n' \
        "$(git rev-parse --short "$base")" "$(git rev-parse --short "$remote")" \
        "$(git rev-parse --short "$head")" >&2
    printf '%s\n' "$base"
}

self_test() {
    work=$(mktemp -d "${TMPDIR:-/tmp}/merge-base-test.XXXXXX")
    trap 'rm -rf "$work"' EXIT
    here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
    script=$here/merge-base.sh
    spec_check=$here/check-spec-archives.sh
    readme_check=$here/check-readme-translation.sh
    remote_repo=$work/remote.git
    passed=0

    identity() {
        git config user.email test@example.invalid
        git config user.name test
    }
    # A new top entry of the specification log.
    add_entry() { # add_entry HEADING [RULES-LINE]
        { awk '/^## / { exit } { print }' spec/log.md
          printf '## %s\n\n%s\n\nSummary: Amend.\n\n' "$1" "${2:-Rules: X-2}"
          awk 'found || /^## / { found = 1; print }' spec/log.md; } > spec/log.md.new
        mv spec/log.md.new spec/log.md
    }
    amend_spec() { printf 'amended by %s\n' "$1" >> spec/kernel-spec.md; }
    # main gains an amendment to the specification and both READMEs, as another
    # branch's merge would add them.
    upstream_advances() { # upstream_advances N
        (
            cd "$work/upstream"
            amend_spec "upstream $1"
            add_entry "2026-02-0$1 Upstream $1"
            printf 'upstream %s\n' "$1" >> README.md
            printf 'upstream %s\n' "$1" >> README.zh-CN.md
            git add -A
            git commit -q -m "upstream $1"
            git push -q origin main
        )
    }
    # Run the checks the Makefile runs, with the base this script chooses.
    chain() {
        base=$(env -u GITHUB_ACTIONS sh "$script" 2>"$work/chain-err") || {
            cat "$work/chain-err"
            return 1
        }
        cat "$work/chain-err"
        sh "$spec_check" "$base" || return 1
        sh "$readme_check" "$base" || return 1
        sh "$spec_check" --require-approval "$base" || return 1
    }
    local_script() { env -u GITHUB_ACTIONS sh "$script"; }
    ci_script() { env GITHUB_ACTIONS=true sh "$script"; }
    expect() { # expect pass|reject PATTERN DESCRIPTION COMMAND...
        want=$1; pattern=$2; what=$3
        shift 3
        outcome=pass
        "$@" > "$work/out" 2>&1 || outcome=reject
        if test "$outcome" != "$want" || ! grep -q -- "$pattern" "$work/out"; then
            echo "merge-base self-test: $what: expected $want matching '$pattern', got $outcome:" >&2
            cat "$work/out" >&2
            exit 1
        fi
        passed=$((passed + 1))
    }
    # The commit the script prints is exactly the expected one, alone on stdout.
    expect_base() { # expect_base EXPECTED-COMMIT DESCRIPTION
        got=$(local_script 2>/dev/null) || {
            echo "merge-base self-test: $2: the script failed" >&2
            exit 1
        }
        if test "$got" != "$1"; then
            echo "merge-base self-test: $2: expected base $1, got $got" >&2
            exit 1
        fi
        passed=$((passed + 1))
    }
    branch() { # branch NAME START: a clean branch of the dev clone
        cd "$work/dev"
        git reset -q --hard
        git clean -q -f -d
        git checkout -q -B "$1" "$2"
    }
    commit_other() {
        printf 'edit\n' >> other.md
        git add -A
        git commit -q -m "other $1"
    }

    git init -q -b main "$work/upstream"
    cd "$work/upstream"
    identity
    mkdir spec
    printf '# Kernel Specification\nrules 1\nrules 2\n' > spec/kernel-spec.md
    printf '# Kernel Specification v0.0\nzero\n' > spec/kernel-spec-v0.0.md
    printf '# Specification change log\n\nPreface.\n\n## 2026-01-01 v0.1: Base\n\nRules: X-1\n\nSummary: Base.\n' > spec/log.md
    printf 'english\n' > README.md
    printf 'chinese\n' > README.zh-CN.md
    printf 'other\n' > other.md
    git add -A
    git commit -q -m base
    git init -q --bare -b main "$remote_repo"
    git remote add origin "$remote_repo"
    git push -q origin main
    git clone -q "$remote_repo" "$work/dev"
    cd "$work/dev"
    identity
    m0=$(git rev-parse origin/main)

    # A: the local main and the clone's origin/main are both stale; main moved
    # on and the branch has merged it, changing nothing of the specification.
    # The base is main's tip, and nothing of what main gained is charged.
    branch feature-a main
    commit_other a
    upstream_advances 1
    cd "$work/dev"
    git fetch -q origin
    git merge -q origin/main -m 'merge upstream'
    tip=$(git rev-parse origin/main)
    git update-ref refs/remotes/origin/main "$m0"
    test "$(git rev-parse main)" = "$m0" || { echo 'merge-base self-test: the local main was meant to be stale' >&2; exit 1; }
    expect pass "unchanged since $(git rev-parse --short "$tip")" \
        'stale local main and stale origin/main, branch merged newer main' chain
    expect_base "$tip" 'the base of a branch that merged main is main at the merge'
    test "$(git rev-parse origin/main)" = "$tip" || { echo 'merge-base self-test: the fetch did not update origin/main' >&2; exit 1; }
    passed=$((passed + 1))

    # B: main advanced after the fork and the branch did not merge it.
    git push -q origin feature-a
    upstream_advances 2
    cd "$work/dev"
    branch feature-b origin/main
    fork=$(git rev-parse HEAD)
    commit_other b
    upstream_advances 3
    cd "$work/dev"
    expect pass 'unchanged since' 'main advanced after the fork' chain
    expect_base "$fork" 'the base of a branch that did not merge main is its fork point'

    # C: the branch is already contained in main.
    git fetch -q origin
    branch feature-c origin/main
    expect pass 'unchanged since' 'a branch contained in main' chain
    expect_base "$(git rev-parse HEAD)" 'the base of a branch contained in main is the branch'

    # D: a branch that amends the specification is still held to the rules.
    branch feature-d origin/main
    amend_spec d
    expect reject 'gained 0' 'an amendment with no log entry' chain
    add_entry '2026-02-10 Branch amendment'
    expect pass 'is new and complete' 'an amendment with its log entry' chain
    add_entry '2026-02-11 Second entry'
    expect reject 'gained 2' 'an amendment with two log entries' chain
    branch feature-d2 origin/main
    amend_spec d2
    add_entry '2026-02-10 Branch amendment' 'Rules:'
    expect reject 'nonempty Rules:' 'an amendment whose entry names no rule' chain
    branch feature-d3 origin/main
    amend_spec d3
    add_entry '2026-02-10 Branch amendment'
    cp spec/kernel-spec.md spec/kernel-spec-v0.1.md
    expect reject 'adds no archive' 'an amendment that adds an archive' chain
    branch feature-d4 origin/main
    printf 'edited\n' >> spec/kernel-spec-v0.0.md
    expect reject 'released specifications changed' 'a modified released archive' chain
    branch feature-d5 origin/main
    printf 'edited\n' >> README.md
    expect reject 'did not' 'a README change without its translation' chain

    # E: outside CI an unreachable origin is an error, not a stale base.
    branch feature-e origin/main
    git remote set-url origin "$work/missing.git"
    expect reject 'cannot fetch main from origin' 'an unreachable origin outside CI' local_script
    expect reject 'cannot fetch main from origin' 'GITHUB_ACTIONS other than true is not CI' \
        env GITHUB_ACTIONS=false sh "$script"
    # F: CI does not fetch and reads the ref it already has.
    expect pass 'review base:' 'CI with an unreachable origin' ci_script
    git remote set-url origin "$remote_repo"

    # G: no origin/main, and no merge base, are errors.
    git init -q -b main "$work/bare-checkout"
    (
        cd "$work/bare-checkout"
        identity
        git commit -q --allow-empty -m first
    )
    cd "$work/bare-checkout"
    expect reject 'refs/remotes/origin/main does not exist' 'no origin/main in CI' ci_script
    git update-ref refs/remotes/origin/main "$(git commit-tree "$(git hash-object -t tree /dev/null)" -m unrelated)"
    expect reject 'no merge base' 'histories with no merge base' ci_script

    # H: a second clone of the same branch gets the same base.
    git clone -q -b feature-a "$remote_repo" "$work/second" 2>/dev/null
    cd "$work/second"
    second=$(local_script 2>/dev/null)
    cd "$work/dev"
    git checkout -q feature-a
    first=$(local_script 2>/dev/null)
    test "$first" = "$second" || {
        echo "merge-base self-test: two clones of one branch chose $first and $second" >&2
        exit 1
    }
    passed=$((passed + 1))

    echo "merge-base self-test: $passed cases pass"
}

case "${1:-}" in
    --self-test) test "$#" -eq 1 || usage; self_test ;;
    '') resolve ;;
    *) usage ;;
esac
