#!/bin/sh
# Released specification archives and the amendment that adds one.
#
# The working tree is compared with its merge base on MAIN-REF, which is what
# a merge would change in main: an archive main added after this branch's
# fork is not a deletion here. Rules:
#   - no released spec/kernel-spec-v*.md is modified, renamed or removed;
#   - an unchanged active specification adds no archive;
#   - a changed active specification adds exactly the outgoing version's
#     archive, byte-identical to the base's active file, and retitles the
#     active file with the next version (vA.B+1, or vA+1.0).
# A green run says only that; it does not judge the amendment's content.
# `--self-test` builds throwaway repositories and requires each rule to
# accept its valid case and reject its invalid ones for the intended reason.
set -eu
LC_ALL=C
export LC_ALL

TITLE='# Kernel Specification '

usage() {
    echo 'usage: check-spec-archives.sh MAIN-REF | --self-test' >&2
    exit 2
}

fail() {
    printf 'spec archives: %s\n' "$*" >&2
    exit 1
}

# Version token (for example v0.69) from a specification's first line.
token_of() {
    head -n 1 | sed -n "s/^$TITLE\\(v[0-9][0-9]*\\.[0-9][0-9]*\\)\$/\\1/p"
}

# awk reads the parts as decimal, so a zero-padded minor stays decimal.
successor_of() {
    printf '%s\n' "${1#v}" | awk -F . '{ printf "v%d.%d v%d.0\n", $1, $2 + 1, $1 + 1 }'
}

check() {
    main_ref=$1
    git rev-parse --verify --quiet "$main_ref^{commit}" >/dev/null ||
        fail "$main_ref must name a commit (CI creates a local main ref)"
    base=$(git merge-base "$main_ref" HEAD) || fail "no merge base with $main_ref"
    short=$(git rev-parse --short "$base")

    changed=$(git diff --name-status --diff-filter=MDRCT "$base" -- 'spec/kernel-spec-v*.md')
    test -z "$changed" ||
        fail "released specifications changed since $short:
$changed"

    listed=$(mktemp "${TMPDIR:-/tmp}/spec-archives.XXXXXX")
    trap 'rm -f "$listed" "$listed.base"' EXIT
    git ls-tree --name-only "$base" -- spec/ | grep '^spec/kernel-spec-v.*\.md$' |
        sort > "$listed.base" || :
    for file in spec/kernel-spec-v*.md; do
        test -e "$file" && printf '%s\n' "$file"
    done | sort | comm -13 "$listed.base" - > "$listed"
    added=$(cat "$listed")

    if git diff --quiet "$base" -- spec/kernel-spec.md; then
        test -z "$added" ||
            fail "the active specification is unchanged since $short, but archives were added:
$added"
        echo "spec archives: no released archive changed; the active specification is unchanged since $short"
        return
    fi

    outgoing=$(git show "$base:spec/kernel-spec.md" | token_of)
    test -n "$outgoing" || fail "$short:spec/kernel-spec.md has no '${TITLE}vA.B' title"
    incoming=$(token_of < spec/kernel-spec.md)
    test -n "$incoming" || fail "spec/kernel-spec.md has no '${TITLE}vA.B' title"
    archive="spec/kernel-spec-$outgoing.md"

    test "$added" = "$archive" ||
        fail "amending $outgoing adds exactly $archive; added archives are:
${added:-(none)}"
    git show "$base:spec/kernel-spec.md" | cmp -s - "$archive" ||
        fail "$archive differs from the outgoing $outgoing bytes at $short"
    case " $(successor_of "$outgoing") " in
        *" $incoming "*) ;;
        *) fail "the amended title is $incoming; the version after $outgoing is $(successor_of "$outgoing" | sed 's/ / or /')" ;;
    esac
    echo "spec archives: $outgoing archived byte for byte and the active specification retitled $incoming; no released archive changed since $short"
}

self_test() {
    work=$(mktemp -d "${TMPDIR:-/tmp}/spec-archives-test.XXXXXX")
    trap 'rm -rf "$work"' EXIT
    script=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/$(basename -- "$0")
    git init -q -b main "$work/repo"
    cd "$work/repo"
    git config user.email test@example.invalid
    git config user.name test
    mkdir spec
    printf '%sv0.1\nrules one\n' "$TITLE" > spec/kernel-spec.md
    printf '%sv0.0\nrules zero\n' "$TITLE" > spec/kernel-spec-v0.0.md
    git add -A && git commit -qm base

    passed=0
    test "$(successor_of v0.69)" = 'v0.70 v1.0' &&
        test "$(successor_of v1.08)" = 'v1.9 v2.0' || {
        echo 'spec archives self-test: successor_of reads versions as decimal' >&2
        exit 1
    }
    expect() { # expect pass|reject PATTERN DESCRIPTION
        outcome=pass
        sh "$script" main > "$work/out" 2>&1 || outcome=reject
        if test "$outcome" != "$1" || ! grep -q -- "$2" "$work/out"; then
            echo "spec archives self-test: $3: expected $1 matching '$2', got $outcome:" >&2
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
    amend() { # amend OUTGOING-ARCHIVE-NAME NEW-TITLE
        git show main:spec/kernel-spec.md > "spec/$1"
        printf '%s%s\nrules amended\n' "$TITLE" "$2" > spec/kernel-spec.md
    }

    branch unchanged
    expect pass 'is unchanged' 'an unchanged branch'
    amend kernel-spec-v0.1.md v0.2
    expect pass 'v0.1 archived byte for byte' 'an uncommitted amendment'
    git add -A && git commit -qm amend
    expect pass 'retitled v0.2' 'a committed amendment'

    branch major
    amend kernel-spec-v0.1.md v1.0
    expect pass 'retitled v1.0' 'a major-version amendment'

    branch missing
    printf '%sv0.2\nrules amended\n' "$TITLE" > spec/kernel-spec.md
    expect reject 'adds exactly spec/kernel-spec-v0.1.md' 'a missing archive'

    branch misnamed
    amend kernel-spec-v1.md v0.2
    expect reject 'adds exactly spec/kernel-spec-v0.1.md' 'an archive under the wrong name'

    branch edited
    amend kernel-spec-v0.1.md v0.2
    printf 'edited\n' >> spec/kernel-spec-v0.1.md
    expect reject 'differs from the outgoing v0.1 bytes' 'an edited archive'

    branch stale-title
    amend kernel-spec-v0.1.md v0.1
    expect reject 'the version after v0.1 is v0.2 or v1.0' 'an unadvanced title'

    branch skipped
    amend kernel-spec-v0.1.md v0.3
    expect reject 'the amended title is v0.3' 'a skipped version'

    branch extra
    git show main:spec/kernel-spec.md > spec/kernel-spec-v0.1.md
    expect reject 'unchanged since' 'an archive added without an amendment'

    branch modified
    printf 'edited\n' >> spec/kernel-spec-v0.0.md
    expect reject 'released specifications changed' 'a modified released archive'

    branch removed
    git rm -q spec/kernel-spec-v0.0.md
    expect reject 'released specifications changed' 'a removed released archive'

    branch behind
    git reset -q --hard
    git checkout -q main
    amend kernel-spec-v0.1.md v0.2
    git add -A && git commit -qm 'main amends after the fork'
    git checkout -q behind
    expect pass 'is unchanged' 'a branch behind a main that added an archive'

    echo "spec archives self-test: $passed cases pass"
}

case "${1:-}" in
    --self-test) test "$#" -eq 1 || usage; self_test ;;
    ''|-*) usage ;;
    *) test "$#" -eq 1 || usage; check "$1" ;;
esac
