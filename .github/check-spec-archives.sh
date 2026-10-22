#!/bin/sh
# The specification's title, its change log and its released archives.
#
# The working tree is compared with its merge base on MAIN-REF, which is what
# a merge would change in main: an archive or log entry main added after this
# branch's fork is not a deletion here. The specification carries no version,
# so two branches that amend it write no line in common to claim one; the
# version of an entry is derived from spec/log.md (spec-version.sh). Rules:
#   - spec/kernel-spec.md starts with the line `# Kernel Specification` and
#     names no version;
#   - no released spec/kernel-spec-v*.md is modified, renamed or removed;
#   - a changed active specification adds no archive and exactly one
#     spec/log.md entry; an unchanged one adds no entry;
#   - the log keeps every entry of the base byte for byte, below the new one;
#     blank lines around entries are not significant, so the entries a union
#     merge of two branches keeps need no repair;
#   - a new entry names a version only to start the next major version;
#   - an archive is a milestone: it is added with the specification unchanged,
#     named for the version of the newest entry, and is a copy of the active
#     file.
# A green run says only that; it does not judge the amendment's content.
#
# `--require-approval BASE` is the readiness check, named like the tree lint's
# flag of the same name: an active specification changed since BASE needs a
# newest spec/log.md entry that is new since BASE and carries nonempty Rules:
# and Summary: fields, the design tree log's form with Rules: in place of
# Nodes:. A field's value is what follows its name, surrounding spaces
# removed, as the tree lint reads it. The check cannot tell whether Rules:
# names every changed rule.
#
# `--self-test` builds throwaway repositories and requires each rule to
# accept its valid case and reject its invalid ones for the intended reason.
set -eu
LC_ALL=C
export LC_ALL

TITLE='# Kernel Specification'
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

usage() {
    echo 'usage: check-spec-archives.sh MAIN-REF | --require-approval BASE | --self-test' >&2
    exit 2
}

fail() {
    printf 'spec archives: %s\n' "$*" >&2
    exit 1
}

# awk reads the major part as decimal, so a zero-padded one stays decimal.
major_successor_of() {
    printf '%s\n' "${1#v}" | awk -F . '{ printf "v%d.0\n", $1 + 1 }'
}

# The entries of a log on standard input, each without its trailing blank
# lines and followed by a marker line; whatever precedes the first entry is
# dropped. Two logs have the same entries exactly when their output is equal.
canonical_entries() {
    awk '
        function flush(   i) {
            if (!started) return
            while (count > 0 && lines[count] ~ /^[ \t]*$/) count--
            for (i = 1; i <= count; i++) print lines[i]
            print "@@end-of-entry@@"
            count = 0
        }
        /^## / { flush(); started = 1 }
        started { lines[++count] = $0 }
        END { flush() }'
}

check() {
    main_ref=$1
    git rev-parse --verify --quiet "$main_ref^{commit}" >/dev/null ||
        fail "$main_ref must name a commit (CI creates a local main ref)"
    base=$(git merge-base "$main_ref" HEAD) || fail "no merge base with $main_ref"
    short=$(git rev-parse --short "$base")

    test "$(head -n 1 spec/kernel-spec.md)" = "$TITLE" ||
        fail "spec/kernel-spec.md must start with the line '$TITLE' and name no version; its version is derived from spec/log.md by .github/spec-version.sh (a branch written under the earlier procedure drops the version from the title, from its log entry heading and its kernel-spec-vN.md archive)"

    changed=$(git diff --name-status --diff-filter=MDRCT "$base" -- 'spec/kernel-spec-v*.md')
    test -z "$changed" ||
        fail "released specifications changed since $short:
$changed"

    tmp=$(mktemp -d "${TMPDIR:-/tmp}/spec-archives.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT
    git ls-tree --name-only "$base" -- spec/ | grep '^spec/kernel-spec-v.*\.md$' |
        sort > "$tmp/base" || :
    for file in spec/kernel-spec-v*.md; do
        test -e "$file" && printf '%s\n' "$file"
    done | sort | comm -13 "$tmp/base" - > "$tmp/added"
    added=$(cat "$tmp/added")

    test -f spec/log.md || fail "spec/log.md is missing"
    git show "$base:spec/log.md" 2>/dev/null | canonical_entries > "$tmp/base-log" || :
    canonical_entries < spec/log.md > "$tmp/head-log"
    base_count=$(grep -c '^## ' "$tmp/base-log" || :)
    head_count=$(grep -c '^## ' "$tmp/head-log" || :)
    new=$((head_count - base_count))
    test "$new" -ge 0 ||
        fail "spec/log.md has fewer entries than at $short"
    base_lines=$(wc -l < "$tmp/base-log" | tr -d ' ')
    tail -n "$base_lines" "$tmp/head-log" | cmp -s - "$tmp/base-log" ||
        fail "spec/log.md changes or reorders an entry that exists at $short; an entry stays as written and a new one goes first"

    if git diff --quiet "$base" -- spec/kernel-spec.md; then
        test "$new" -eq 0 ||
            fail "the active specification is unchanged since $short, but spec/log.md gained $new entries"
        if test -z "$added"; then
            echo "spec archives: no released archive changed; the active specification is unchanged since $short"
            return
        fi
        version=$(sh "$HERE/spec-version.sh" spec/log.md) || exit 1
        archive="spec/kernel-spec-$version.md"
        test "$added" = "$archive" ||
            fail "an archive of a milestone adds exactly $archive, named for the version of the newest log entry; added archives are:
$added"
        cmp -s spec/kernel-spec.md "$archive" ||
            fail "$archive differs from the active specification"
        echo "spec archives: $version archived byte for byte; the active specification is unchanged since $short"
        return
    fi

    test -z "$added" ||
        fail "the active specification changed since $short, and an amendment adds no archive; added archives are:
$added"
    test "$new" -eq 1 ||
        fail "the active specification changed since $short, so spec/log.md must gain exactly one entry; it gained $new"
    heading=$(grep -m 1 '^## ' "$tmp/head-log")
    printf '%s\n' "$heading" | grep -q '^## [0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\} [^ ]' ||
        fail "the new spec/log.md heading must be '## YYYY-MM-DD <title>'"
    named=$(printf '%s\n' "$heading" | awk '$3 ~ /^v[0-9]+\.[0-9]+:?$/ { sub(/:$/, "", $3); print $3 }')
    if test -n "$named"; then
        git show "$base:spec/log.md" > "$tmp/base-log.md"
        previous=$(sh "$HERE/spec-version.sh" "$tmp/base-log.md") || exit 1
        allowed=$(major_successor_of "$previous")
        test "$named" = "$allowed" ||
            fail "the new spec/log.md entry names $named; an entry names a version only to start the next major version ($allowed after $previous), and every other version is derived from the log"
    fi
    echo "spec archives: the active specification changed since $short with no version in its title, no archive and one new log entry; no released archive changed"
}

readiness() {
    base=$1
    git rev-parse --verify --quiet "$base^{commit}" >/dev/null ||
        fail "$base must name a commit"
    short=$(git rev-parse --short "$base")
    if git diff --quiet "$base" -- spec/kernel-spec.md; then
        echo "spec readiness: the active specification is unchanged since $short"
        return
    fi
    test -f spec/log.md ||
        fail "the active specification changed since $short, but spec/log.md is missing"
    heading=$(grep -m 1 '^## ' spec/log.md) ||
        fail "the active specification changed since $short, but spec/log.md has no entry"
    printf '%s\n' "$heading" | grep -q '^## [0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\} [^ ]' ||
        fail "the newest spec/log.md heading must be '## YYYY-MM-DD <title>'"
    if git show "$base:spec/log.md" 2>/dev/null | grep -qxF -- "$heading"; then
        fail "the active specification changed since $short, but the newest spec/log.md entry is not new"
    fi
    entry=$(awk 'found && /^## / { exit } /^## / { found = 1 } found' spec/log.md)
    for field in Rules: Summary:; do
        printf '%s\n' "$entry" | grep -q "^$field[[:space:]]*[^[:space:]]" ||
            fail "the newest spec/log.md entry lacks a nonempty $field field"
    done
    echo "spec readiness: the active specification changed since $short and the newest spec/log.md entry is new and complete"
}

self_test() {
    work=$(mktemp -d "${TMPDIR:-/tmp}/spec-archives-test.XXXXXX")
    trap 'rm -rf "$work"' EXIT
    script=$HERE/$(basename -- "$0")
    git init -q -b main "$work/repo"
    cd "$work/repo"
    git config user.email test@example.invalid
    git config user.name test
    mkdir spec
    printf 'spec/log.md merge=union\n' > .gitattributes
    printf '%s\nrules 1\nrules 2\nrules 3\nrules 4\nrules 5\nrules 6\n' "$TITLE" > spec/kernel-spec.md
    printf '%s v0.0\nrules zero\n' "$TITLE" > spec/kernel-spec-v0.0.md
    printf '# Specification change log\n\nPreface.\n\n## 2026-01-01 v0.1: Base\n\nRules: X-1\n\nSummary: Base.\n' > spec/log.md
    git add -A && git commit -qm base

    passed=0
    test "$(major_successor_of v0.69)" = 'v1.0' &&
        test "$(major_successor_of v1.08)" = 'v2.0' || {
        echo 'spec archives self-test: major_successor_of reads versions as decimal' >&2
        exit 1
    }
    expect() { # expect pass|reject PATTERN DESCRIPTION [OPTION]
        outcome=pass
        sh "$script" ${4:-} main > "$work/out" 2>&1 || outcome=reject
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
    amend() { # amend LINE: change one line of the rules, keeping the title
        awk -v n="$1" 'NR == n { print "amended " n; next } { print }' spec/kernel-spec.md > spec/kernel-spec.md.new
        mv spec/kernel-spec.md.new spec/kernel-spec.md
    }
    entry() { # entry HEADING RULES-LINE SUMMARY-LINE [GLUE]: a new top entry
        awk '/^## / { exit } { print }' spec/log.md > "$work/preface"
        awk 'found || /^## / { found = 1; print }' spec/log.md > "$work/entries"
        { cat "$work/preface"
          printf '## %s\n\n%s\n\n%s\n' "$1" "$2" "$3"
          test "${4:-}" = glue || printf '\n'
          cat "$work/entries"; } > spec/log.md.new
        mv spec/log.md.new spec/log.md
    }
    amend_logged() { amend "$1"; entry "${2:-2026-01-02 Second}" 'Rules: X-2' 'Summary: Amend.'; }

    branch unchanged
    expect pass 'is unchanged' 'an unchanged branch'
    branch amended
    amend_logged 2
    expect pass 'one new log entry' 'an uncommitted amendment'
    git add -A && git commit -qm amend
    expect pass 'one new log entry' 'a committed amendment'

    branch major
    amend_logged 2 '2026-01-02 v1.0: Major'
    expect pass 'one new log entry' 'an entry that starts the next major version'
    branch minor
    amend_logged 2 '2026-01-02 v0.9: Minor'
    expect reject 'names v0.9; an entry names a version only to start the next major version (v1.0 after v0.1)' 'an entry that names a minor version'
    branch skipped-major
    amend_logged 2 '2026-01-02 v2.0: Skipped'
    expect reject 'names v2.0' 'an entry that skips a major version'
    branch undated
    amend_logged 2 'Second'
    expect reject "must be '## YYYY-MM-DD <title>'" 'an entry heading without a date'

    branch versioned-title
    amend_logged 2
    awk 'NR == 1 { print "# Kernel Specification v0.2"; next } { print }' spec/kernel-spec.md > spec/kernel-spec.md.new
    mv spec/kernel-spec.md.new spec/kernel-spec.md
    expect reject "must start with the line '# Kernel Specification'" 'a title that names a version'
    branch untitled
    amend_logged 2
    awk 'NR == 1 { print "# Specification"; next } { print }' spec/kernel-spec.md > spec/kernel-spec.md.new
    mv spec/kernel-spec.md.new spec/kernel-spec.md
    expect reject "must start with the line" 'a different title'

    branch archived-amendment
    amend_logged 2
    git show main:spec/kernel-spec.md > spec/kernel-spec-v0.1.md
    expect reject 'an amendment adds no archive' 'an archive added with an amendment'
    branch unlogged
    amend 2
    expect reject 'gained 0' 'an amendment with no new entry'
    branch two-entries
    amend_logged 2
    entry '2026-01-03 Third' 'Rules: X-3' 'Summary: Again.'
    expect reject 'gained 2' 'an amendment with two new entries'
    branch entry-only
    entry '2026-01-02 Second' 'Rules: X-2' 'Summary: Amend.'
    expect reject 'unchanged since .*gained 1 entries' 'an entry with no amendment'

    branch edited-entry
    amend_logged 2
    awk '{ if ($0 == "Summary: Base.") print "Summary: Edited."; else print }' spec/log.md > spec/log.md.new
    mv spec/log.md.new spec/log.md
    expect reject 'changes or reorders an entry' 'an edited old entry'
    branch removed-entry
    amend_logged 2
    entry '2026-01-03 Third' 'Rules: X-3' 'Summary: Again.'
    awk 'BEGIN { skip = 0 } /^## 2026-01-01/ { skip = 1 } !skip { print }' spec/log.md > spec/log.md.new
    mv spec/log.md.new spec/log.md
    expect reject 'changes or reorders an entry' 'a removed old entry'
    branch entry-below
    amend 2
    { cat spec/log.md; printf '\n## 2026-01-02 Second\n\nRules: X-2\n\nSummary: Amend.\n'; } > spec/log.md.new
    mv spec/log.md.new spec/log.md
    expect reject 'changes or reorders an entry' 'a new entry below an old one'
    branch preface-edit
    awk '{ if ($0 == "Preface.") print "Preface, reworded."; else print }' spec/log.md > spec/log.md.new
    mv spec/log.md.new spec/log.md
    expect pass 'is unchanged' 'a reworded preface'
    branch glued
    amend 2
    entry '2026-01-02 Second' 'Rules: X-2' 'Summary: Amend.' glue
    expect pass 'one new log entry' 'a new entry with no blank line before the old heading'

    branch milestone
    git show main:spec/kernel-spec.md > spec/kernel-spec-v0.1.md
    expect pass 'v0.1 archived byte for byte' 'a milestone archive'
    branch misnamed-milestone
    git show main:spec/kernel-spec.md > spec/kernel-spec-v1.md
    expect reject 'adds exactly spec/kernel-spec-v0.1.md' 'a milestone archive under the wrong name'
    branch edited-milestone
    { git show main:spec/kernel-spec.md; printf 'edited\n'; } > spec/kernel-spec-v0.1.md
    expect reject 'differs from the active specification' 'an edited milestone archive'

    branch modified
    printf 'edited\n' >> spec/kernel-spec-v0.0.md
    expect reject 'released specifications changed' 'a modified released archive'
    branch removed
    git rm -q spec/kernel-spec-v0.0.md
    expect reject 'released specifications changed' 'a removed released archive'

    # A branch behind a main that amended is not charged with main's entry.
    branch behind
    git checkout -q main
    amend_logged 2
    git add -A && git commit -qm 'main amends after the fork'
    git checkout -q behind
    expect pass 'is unchanged' 'a branch behind a main that amended'
    git checkout -q main
    git reset -q --hard HEAD~1

    # Two branches amend different lines and add an entry each. The second to
    # merge takes main's entry and keeps its own on top, with no conflict.
    branch first
    amend_logged 2 '2026-01-02 First'
    git add -A && git commit -qm first
    branch second
    amend_logged 6 '2026-01-03 Second'
    git add -A && git commit -qm second
    git checkout -q main && git merge -q --ff-only first
    git checkout -q second
    git merge -q main -m 'merge main' > "$work/merge" 2>&1 || {
        echo 'spec archives self-test: merging main into the second branch conflicted:' >&2
        cat "$work/merge" >&2
        exit 1
    }
    expect pass 'one new log entry' 'a merge of two branches that each added an entry'
    test "$(grep -m 1 '^## ' spec/log.md)" = '## 2026-01-03 Second' &&
        test "$(grep -c '^## ' spec/log.md)" -eq 3 || {
        echo 'spec archives self-test: the union merge did not keep both entries, the branch first:' >&2
        cat spec/log.md >&2
        exit 1
    }
    git checkout -q main
    git reset -q --hard HEAD~1

    log_entry() { # log_entry RULES-LINE SUMMARY-LINE
        entry '2026-01-02 Amend' "$1" "$2"
    }

    branch readiness-unchanged
    expect pass 'is unchanged' 'readiness with an unchanged specification' --require-approval
    branch readiness-unlogged
    amend 2
    expect reject 'is not new' 'readiness with an amendment and no new entry' --require-approval
    branch readiness-logged
    amend 2
    log_entry 'Rules: X-1' 'Summary: Amend.'
    expect pass 'is new and complete' 'readiness with an entry that has no approval line' --require-approval
    branch readiness-empty-rules
    amend 2
    log_entry 'Rules:' 'Summary: Amend.'
    expect reject 'nonempty Rules:' 'readiness with an empty Rules: field' --require-approval
    branch readiness-no-summary
    amend 2
    log_entry 'Rules: X-1' 'Notes: none'
    expect reject 'nonempty Summary:' 'readiness with no Summary: field' --require-approval
    branch readiness-spaced
    amend 2
    log_entry 'Rules:   X-1' 'Summary:   Amend.'
    expect pass 'is new and complete' 'readiness with spaces after the field names' --require-approval
    branch readiness-blank
    amend 2
    log_entry 'Rules:   ' 'Summary: Amend.'
    expect reject 'nonempty Rules:' 'readiness with a blank Rules: field' --require-approval

    echo "spec archives self-test: $passed cases pass"
}

case "${1:-}" in
    --self-test) test "$#" -eq 1 || usage; self_test ;;
    --require-approval) test "$#" -eq 2 || usage; readiness "$2" ;;
    ''|-*) usage ;;
    *) test "$#" -eq 1 || usage; check "$1" ;;
esac
