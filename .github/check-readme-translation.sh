#!/bin/sh
# README.zh-CN.md translates README.md, and a change to one of them is a
# change to both (AGENTS.md).
#
# The working tree is compared with its merge base on MAIN-REF, which is what
# a merge would change in main, and a change to exactly one of the two files
# is refused. A base without the translation is the change that adds it and
# is not judged. The check sees which files changed, not whether the
# translation says what README.md says; review judges that.
# `--self-test` builds a throwaway repository and requires each case to be
# judged as intended.
set -eu
LC_ALL=C
export LC_ALL

ENGLISH=README.md
CHINESE=README.zh-CN.md

usage() {
    echo 'usage: check-readme-translation.sh MAIN-REF | --self-test' >&2
    exit 2
}

fail() {
    printf 'readme translation: %s\n' "$*" >&2
    exit 1
}

check() {
    main_ref=$1
    git rev-parse --verify --quiet "$main_ref^{commit}" >/dev/null ||
        fail "$main_ref must name a commit (CI creates a local main ref)"
    base=$(git merge-base "$main_ref" HEAD) || fail "no merge base with $main_ref"
    short=$(git rev-parse --short "$base")

    if ! git cat-file -e "$base:$CHINESE" 2>/dev/null; then
        echo "readme translation: $CHINESE does not exist at $short, so there is nothing to pair yet"
        return
    fi
    english=changed
    chinese=changed
    git diff --quiet "$base" -- "$ENGLISH" && english=unchanged
    git diff --quiet "$base" -- "$CHINESE" && chinese=unchanged
    case "$english $chinese" in
        'changed unchanged')
            fail "$ENGLISH changed since $short but $CHINESE did not; update the translation in the same change" ;;
        'unchanged changed')
            fail "$CHINESE changed since $short but $ENGLISH did not; the translation follows $ENGLISH, so change both together" ;;
    esac
    echo "readme translation: $ENGLISH and $CHINESE are both $english since $short"
}

self_test() {
    work=$(mktemp -d "${TMPDIR:-/tmp}/readme-translation-test.XXXXXX")
    trap 'rm -rf "$work"' EXIT
    script=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/$(basename -- "$0")
    git init -q -b main "$work/repo"
    cd "$work/repo"
    git config user.email test@example.invalid
    git config user.name test
    printf 'english\n' > "$ENGLISH"
    printf 'other\n' > other.md
    git add -A && git commit -qm base

    passed=0
    expect() { # expect pass|reject PATTERN DESCRIPTION
        outcome=pass
        sh "$script" main > "$work/out" 2>&1 || outcome=reject
        if test "$outcome" != "$1" || ! grep -q -- "$2" "$work/out"; then
            echo "readme translation self-test: $3: expected $1 matching '$2', got $outcome:" >&2
            cat "$work/out" >&2
            exit 1
        fi
        passed=$((passed + 1))
    }
    branch() {
        git reset -q --hard
        git clean -q -f -d
        git checkout -q -B "$1" main
    }

    branch english-before-translation
    printf 'edited\n' >> "$ENGLISH"
    expect pass 'does not exist at' 'an English change before any translation exists'

    branch introduce
    printf 'chinese\n' > "$CHINESE"
    git add -A && git commit -qm 'add the translation'
    expect pass 'does not exist at' 'the change that adds the translation'
    git checkout -q main
    git merge -q --ff-only introduce

    branch neither
    printf 'edited\n' >> other.md
    expect pass 'both unchanged' 'a change to neither file'

    branch both
    printf 'edited\n' >> "$ENGLISH"
    printf 'edited\n' >> "$CHINESE"
    expect pass 'both changed' 'an uncommitted change to both'
    git add -A && git commit -qm both
    expect pass 'both changed' 'a committed change to both'

    branch english-only
    printf 'edited\n' >> "$ENGLISH"
    expect reject "$CHINESE did not" 'an English change alone'
    git add -A && git commit -qm english
    expect reject "$CHINESE did not" 'a committed English change alone'

    branch chinese-only
    printf 'edited\n' >> "$CHINESE"
    expect reject "$ENGLISH did not" 'a Chinese change alone'

    branch removed
    git rm -q "$CHINESE"
    expect reject "$ENGLISH did not" 'the translation removed alone'

    branch behind
    git checkout -q main
    printf 'edited\n' >> "$ENGLISH"
    printf 'edited\n' >> "$CHINESE"
    git add -A && git commit -qm 'main changes both after the fork'
    git checkout -q behind
    expect pass 'both unchanged' 'a branch behind a main that changed both'

    echo "readme translation self-test: $passed cases pass"
}

case "${1:-}" in
    --self-test) test "$#" -eq 1 || usage; self_test ;;
    ''|-*) usage ;;
    *) test "$#" -eq 1 || usage; check "$1" ;;
esac
