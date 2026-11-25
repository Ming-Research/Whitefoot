#!/bin/sh
# Check where compiler/Makefile builds the runtime's test programs and when it
# builds them again. Those programs sit in one directory and make runs one
# that is newer than its sources, so a directory that two checkouts share
# would let one run the other's program. This reads the directory and plans
# builds with `make -n`; nothing is compiled.
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
root=$(dirname "$here")
work=$(mktemp -d "${TMPDIR:-/tmp}/whitefoot-completion-scratch.XXXXXX")
trap 'rm -rf "$work"' EXIT
work=$(CDPATH= cd -- "$work" && pwd -P)
tab=$(printf '\t')

fail() {
    echo "test-completion-scratch: $1" >&2
    exit 1
}

# A second makefile, read after the compiler's, prints the directory as make
# resolves it, and holds a target whose plan shows whether a dry run can tell
# a program that is up to date from one that must be built again.
cat > "$work/show.mk" <<MK
show:
${tab}@printf '%s\\n' '\$(COMPLETION_TMP)'
\$(COMPLETION_TMP)/control: Makefile
${tab}@echo rebuilt
MK

# make_in DIR [NAME=VALUE ...] -- ARGS: make in DIR/compiler, with the caller's
# make state and the two variables under test taken out of the environment.
make_in() {
    dir=$1
    shift
    set_vars=
    while [ "$1" != -- ]; do
        set_vars="$set_vars $1"
        shift
    done
    shift
    # shellcheck disable=SC2086
    env -u MAKEFLAGS -u MAKELEVEL -u MFLAGS -u COMPLETION_TMP -u WHITEFOOT_SCRATCH_ROOT \
        $set_vars make -C "$dir/compiler" -f "$dir/compiler/Makefile" -f "$work/show.mk" "$@"
}
resolved() {
    make_in "$@" -- -s show
}

want=$root/compiler/.scratch/completion-test

# The default belongs to the checkout, whatever the scratch root says.
got=$(resolved "$root" "WHITEFOOT_SCRATCH_ROOT=$work/scratch")
[ "$got" = "$want" ] || fail "with the scratch root set, the default is '$got', not '$want'"
got=$(resolved "$root")
[ "$got" = "$want" ] || fail "with no scratch root set, the default is '$got', not '$want'"

# A directory a caller names is used as given.
got=$(resolved "$root" "COMPLETION_TMP=$work/explicit")
[ "$got" = "$work/explicit" ] || fail "a named directory resolved to '$got', not '$work/explicit'"

# Two checkouts at different paths do not share a directory.
for side in a b; do
    mkdir -p "$work/$side/compiler"
    cp "$root/compiler/Makefile" "$work/$side/compiler/Makefile"
done
got_a=$(resolved "$work/a")
got_b=$(resolved "$work/b")
[ "$got_a" = "$work/a/compiler/.scratch/completion-test" ] || fail "checkout a resolved to '$got_a'"
[ "$got_b" = "$work/b/compiler/.scratch/completion-test" ] || fail "checkout b resolved to '$got_b'"

# In a directory a caller shares, a program made under another configuration
# is built again even when it is newer than every source. The concurrent map's
# four test programs are the ones that once ignored the stamp that records the
# configuration. The control has the same age and no stamp: a dry run must plan
# nothing for it, or the dates in this fixture do not make a program look newer
# than its sources and the rest of the case shows nothing.
shared=$work/shared
mkdir -p "$shared"
programs="concurrent-map-test-copy-out concurrent-map-test-locked concurrent-map-test-collide concurrent-map-test-ignore-activity"
targets=
for name in $programs control; do
    : > "$shared/$name"
    touch -t 203012312359 "$shared/$name"
    targets="$targets $shared/$name"
done
plan=$(make_in "$root" "COMPLETION_TMP=$shared" -- -n $targets 2>&1) || fail "the dry run failed: $plan"
for name in $programs; do
    printf '%s\n' "$plan" | grep -F -- "-o \"$shared/$name\"" > /dev/null \
        || fail "a dry run plans no build of $name in a shared directory"
done
if printf '%s\n' "$plan" | grep -F 'echo rebuilt' > /dev/null; then
    fail "the control program, newer than its sources and with no stamp, is planned for a build"
fi

echo 'test-completion-scratch: ok'
