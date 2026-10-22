#!/bin/sh
# The specification's version, derived from the change log.
#
#   spec-version.sh [LOG]           the active version: the newest entry's
#   spec-version.sh --list [LOG]    every entry newest first, as VERSION TAB
#                                   HEADING (the heading without its `## `)
#   spec-version.sh --self-test
#
# LOG is a spec/log.md file (default spec/log.md, `-` reads standard input).
# `spec/kernel-spec.md` carries no version, so a branch never claims one: the
# version is the position of an entry in the log, which only ever grows at the
# top. An entry heading is `## YYYY-MM-DD <title>`. An entry whose title begins
# `vA.B` names that version (every entry through v0.161 does, and the entry
# that starts a major revision does); any other entry is the next minor version
# after the entry below it, so the entry above v0.161 is v0.162. The version of
# an entry therefore changes only if an entry below it moves, which
# `check-spec-archives.sh` forbids.
set -eu
LC_ALL=C
export LC_ALL

usage() {
    echo 'usage: spec-version.sh [--list] [LOG] | --self-test' >&2
    exit 2
}

fail() {
    printf 'spec version: %s\n' "$*" >&2
    exit 1
}

# Print VERSION TAB HEADING for every entry of the log on standard input,
# newest first.
derive() {
    awk '
        /^## / {
            n++
            heading[n] = substr($0, 4)
            token[n] = ""
            if ($2 !~ /^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/) {
                bad = "spec version: entry heading is not `## YYYY-MM-DD <title>`: " $0
            } else if ($3 ~ /^v[0-9]+\.[0-9]+:?$/) {
                token[n] = $3
                sub(/:$/, "", token[n])
            }
        }
        END {
            if (bad != "") { print bad > "/dev/stderr"; exit 2 }
            if (n == 0) { print "spec version: the log has no entry" > "/dev/stderr"; exit 2 }
            if (token[n] == "") {
                print "spec version: the oldest entry must name its version: " heading[n] > "/dev/stderr"
                exit 2
            }
            for (i = n; i >= 1; i--) {
                if (token[i] != "") {
                    split(substr(token[i], 2), part, ".")
                    major = part[1] + 0
                    minor = part[2] + 0
                } else {
                    minor++
                }
                label[i] = "v" major "." minor
            }
            for (i = 1; i <= n; i++) printf "%s\t%s\n", label[i], heading[i]
        }'
}

run() { # run MODE LOG
    if test "$2" = -; then
        derived=$(derive) || exit 1
    else
        test -f "$2" || fail "$2 does not exist"
        derived=$(derive < "$2") || exit 1
    fi
    if test "$1" = list; then
        printf '%s\n' "$derived"
    else
        printf '%s\n' "$derived" | head -n 1 | cut -f 1
    fi
}

self_test() {
    work=$(mktemp -d "${TMPDIR:-/tmp}/spec-version-test.XXXXXX")
    trap 'rm -rf "$work"' EXIT
    script=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/$(basename -- "$0")
    passed=0
    expect() { # expect pass|reject PATTERN DESCRIPTION [ARGS...]
        outcome=pass
        want=$1 pattern=$2 what=$3
        shift 3
        sh "$script" "$@" > "$work/out" 2>&1 || outcome=reject
        if test "$outcome" != "$want" || ! grep -q -- "$pattern" "$work/out"; then
            echo "spec version self-test: $what: expected $want matching '$pattern', got $outcome:" >&2
            cat "$work/out" >&2
            exit 1
        fi
        passed=$((passed + 1))
    }
    log() { # log HEADING... (newest first); the oldest names its version
        { printf '# Specification change log\n\nPreface.\n\n'
          for heading in "$@"; do printf '## %s\n\nRules: X-1\n\nSummary: Text.\n\n' "$heading"; done
        } > "$work/log.md"
    }

    log '2026-01-03 v0.3: Third' '2026-01-02 v0.2: Second' '2026-01-01 v0.1: First'
    expect pass '^v0.3$' 'the newest entry names its version' "$work/log.md"

    log '2026-01-05 Fifth' '2026-01-04 Fourth' '2026-01-03 v0.3: Third' '2026-01-01 v0.1: First'
    expect pass '^v0.5$' 'unnamed entries count up from the entry below' "$work/log.md"
    expect pass '^v0.5	2026-01-05 Fifth$' 'the list gives the newest entry first' --list "$work/log.md"
    expect pass '^v0.4	2026-01-04 Fourth$' 'the list gives each unnamed entry its count' --list "$work/log.md"
    expect pass '^v0.3	2026-01-03 v0.3: Third$' 'the list keeps a named entry' --list "$work/log.md"
    expect pass '^v0.1	2026-01-01 v0.1: First$' 'the list ends with the oldest entry' --list "$work/log.md"

    log '2026-01-06 Sixth' '2026-01-05 v1.0: Major' '2026-01-04 Fourth' '2026-01-01 v0.3: Third'
    expect pass '^v1.1$' 'a major version restarts the count' "$work/log.md"

    log '2026-01-02 v0.84 amended: Second' '2026-01-01 v0.84: First'
    expect pass '^v0.84$' 'a token followed by a space names the version' "$work/log.md"

    log '2026-01-03 Ten' '2026-01-02 v0.9: Nine'
    expect pass '^v0.10$' 'the minor version counts as a number, not as text' "$work/log.md"

    log '2026-01-02 Second' '2026-01-01 First'
    expect reject 'oldest entry must name its version' 'a log with no named entry' "$work/log.md"

    log 'January Second' '2026-01-01 v0.1: First'
    expect reject 'not `## YYYY-MM-DD <title>`' 'an entry heading without a date' "$work/log.md"

    printf '# Specification change log\n\nPreface only.\n' > "$work/log.md"
    expect reject 'no entry' 'a log with no entries' "$work/log.md"

    expect reject 'does not exist' 'a missing log' "$work/missing.md"

    log '2026-01-02 Second' '2026-01-01 v0.1: First'
    outcome=pass
    sh "$script" - < "$work/log.md" > "$work/out" 2>&1 || outcome=reject
    if test "$outcome" != pass || ! grep -q '^v0.2$' "$work/out"; then
        echo 'spec version self-test: standard input: expected v0.2' >&2
        cat "$work/out" >&2
        exit 1
    fi
    passed=$((passed + 1))

    echo "spec version self-test: $passed cases pass"
}

mode=version
case "${1:-}" in
    --self-test) test "$#" -eq 1 || usage; self_test; exit 0 ;;
    --list) mode=list; shift ;;
    -h|--help) usage ;;
esac
test "$#" -le 1 || usage
run "$mode" "${1:-spec/log.md}"
