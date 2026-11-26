#!/usr/bin/env bash
# Temporary: the evidence of work W291 for the concurrent map test. Removed before the full check.
# usage: tmp-map-overlap.sh calibrate|before|after|swap-control|inert-control|late-control|newtest-control
set -u
ROOT=$(pwd)
S=${RUNNER_TEMP:-/tmp}/map-overlap
mkdir -p "$S"
CC=${CC:-clang}
BASE="-std=c11 -O2 -g -Wall -Wextra -Werror -Wpedantic -pthread -DWF_GUARD_TEST_HOOKS=1"
ASAN="-fsanitize=address,undefined -fno-sanitize-recover=undefined"
TSAN="-fsanitize=thread"
SRC=src/backend/concurrent_map_test.c
N_PLAIN=${N_PLAIN:-50}
N_LOAD=${N_LOAD:-50}
COPIES=${COPIES:-4}
N_CTL=${N_CTL:-5}
export ASAN_OPTIONS=detect_leaks=0
export TSAN_OPTIONS=halt_on_error=1

# build NAME DIR(of compiler/) FLAGS...
build() {
    local name=$1 dir=$2
    shift 2
    echo "== build $name"
    (cd "$dir" && $CC $BASE "$@" $SRC -o "$S/$name") || { echo "BUILD FAILED: $name"; exit 1; }
}

# the first message line a failed run printed
first_line() { grep -h -m1 -E 'concurrent-map-test:|Sanitizer' "$1" | cut -c1-150; }

# one run: prints its seconds and exit status
timed() { # name bin args...
    local name=$1 bin=$2
    shift 2
    local t0=$SECONDS
    "$bin" "$@" > "$S/$name.out" 2>&1
    local status=$?
    echo "$name: exit $status in $((SECONDS - t0)) s; $(first_line "$S/$name.out")"
    return $status
}

# n consecutive runs, a tally of the failures
consecutive() { # name bin n args...
    local name=$1 bin=$2 n=$3 failed=0
    shift 3
    rm -f "$S/$name.tally"
    for i in $(seq "$n"); do
        if ! "$bin" "$@" > "$S/$name.$i.out" 2>&1; then
            failed=$((failed + 1))
            echo "FAIL run $i: $(first_line "$S/$name.$i.out")" >> "$S/$name.tally"
        fi
    done
    echo "$name: $failed of $n runs failed"
    [ -f "$S/$name.tally" ] && cut -d: -f2- "$S/$name.tally" | sort | uniq -c
    return 0
}

# COPIES copies at once, n runs each
loaded() { # name bin n
    local name=$1 bin=$2 n=$3
    rm -f "$S/$name".copy.*.tally
    for c in $(seq "$COPIES"); do
        (
            for i in $(seq "$n"); do
                if ! "$bin" > "$S/$name.$c.$i.out" 2>&1; then
                    echo "FAIL: $(first_line "$S/$name.$c.$i.out")" >> "$S/$name.copy.$c.tally"
                fi
            done
        ) &
    done
    wait
    local failed=0
    [ -n "$(ls "$S/$name".copy.*.tally 2>/dev/null)" ] && failed=$(cat "$S/$name".copy.*.tally | wc -l)
    echo "$name: $failed of $((COPIES * n)) loaded runs failed"
    [ "$failed" -gt 0 ] && cat "$S/$name".copy.*.tally | sort | uniq -c
    return 0
}

# NAME of a build, expecting exit 1 on every one of n runs of args
expect_fail() { # name bin n args...
    local name=$1 bin=$2 n=$3 failed=0
    shift 3
    for i in $(seq "$n"); do
        "$bin" "$@" > "$S/$name.$i.out" 2>&1
        local status=$?
        [ "$status" -ne 0 ] && failed=$((failed + 1))
        echo "$name run $i: exit $status; $(first_line "$S/$name.$i.out")"
    done
    echo "RESULT $name: $failed of $n runs failed"
}

# sources of the merge before this work's changes, in their own tree
old_tree() {
    mkdir -p "$S/old"
    git -C "$ROOT" archive 04ca5968ec9a977d7ee8050abf91601de5561c1e compiler/src/backend | tar -x -C "$S/old"
}

# a copy of this checkout's backend sources
copy_tree() { # dir
    rm -rf "$1"
    mkdir -p "$1"
    cp -R "$ROOT/compiler/src" "$1/src"
}

# the late start of the reader, the holder or the releaser, as LATE names it
late_patch() { # file
    perl -0pi -e '
        sub late { my $w = shift; return "    if (getenv(\"LATE\") != NULL && strcmp(getenv(\"LATE\"), \"$w\") == 0) { struct timespec late_ts = {1, 500000000}; nanosleep(&late_ts, NULL); }\n"; }
        s/(uint64_t absent_length = counted_key\(999, absent\), last = 0;\n)/$1 . late("reader")/e;
        s/(static void \*hold_entries\(void \*arg\) \{\n)/$1 . late("holder")/e;
        s/(static void \*release_reserves\(void \*arg\) \{\n)/$1 . late("releaser")/e;
    ' "$1"
    [ "$(grep -c 'getenv("LATE")' "$1")" -eq 3 ] || { echo "PATCH DID NOT APPLY AS EXPECTED: $1"; grep -n LATE "$1"; exit 1; }
}

# A copy of this checkout with one wrong behaviour patched in, built and run
# N_CTL times on one focused path: how many runs failed, and how many with the
# message the path is to fail with.
variant() { # name file(under src/backend) perl-expression arg expected-message
    local name=$1 file=$2 expr=$3 arg=$4 expected=$5
    local tree="$S/nt-$name-tree"
    copy_tree "$tree"
    perl -0pi -e "$expr" "$tree/src/backend/$file"
    if diff "$ROOT/compiler/src/backend/$file" "$tree/src/backend/$file" > "$S/nt-$name.diff"; then
        echo "PATCH DID NOT APPLY: $name"
        exit 1
    fi
    cat "$S/nt-$name.diff"
    build "nt-$name" "$tree"
    expect_fail "nt-$name" "$S/nt-$name" "$N_CTL" "$arg"
    echo "RESULT nt-$name: $(grep -l -F -- "$expected" "$S"/nt-"$name".*.out 2>/dev/null | wc -l) of $N_CTL runs gave: $expected"
}

case ${1:-} in
calibrate)
    build fixed-plain "$ROOT/compiler"
    for i in 1 2 3; do timed "fixed-plain-$i" "$S/fixed-plain" || true; done
    build fixed-asan "$ROOT/compiler" $ASAN
    timed fixed-asan "$S/fixed-asan" || true
    build fixed-tsan "$ROOT/compiler" $TSAN
    timed fixed-tsan "$S/fixed-tsan" || true
    ;;
before)
    old_tree
    build old-plain "$S/old/compiler"
    timed old-plain-1 "$S/old-plain" || true
    consecutive old-plain "$S/old-plain" "$N_PLAIN"
    loaded old-loaded "$S/old-plain" "$N_LOAD"
    ;;
after)
    build fixed-plain "$ROOT/compiler"
    consecutive fixed-plain "$S/fixed-plain" "$N_PLAIN"
    loaded fixed-loaded "$S/fixed-plain" "$N_LOAD"
    ;;
swap-control)
    copy_tree "$S/swap"
    # The swap without its locks: the two lock steps and the two unlock steps.
    perl -0pi -e 's/(void wf_cmap_swap.*?#undef SWAP_FIELD)/my $f = $1; $f =~ s{    lock_map\(first\);\n    if \(b != a\)\n        lock_map\(first == a \? b : a\);\n}{    (void)first;\n}; $f =~ s{    unlock_map\(a\);\n    if \(b != a\)\n        unlock_map\(b\);\n}{}; $f/se' "$S/swap/src/backend/concurrent_map.c"
    diff "$ROOT/compiler/src/backend/concurrent_map.c" "$S/swap/src/backend/concurrent_map.c"
    [ "$(diff "$ROOT/compiler/src/backend/concurrent_map.c" "$S/swap/src/backend/concurrent_map.c" | grep -c '^[<>]')" -eq 7 ] || { echo "PATCH DID NOT APPLY AS EXPECTED"; exit 1; }
    mkdir -p "$S/swap"
    for v in plain asan tsan; do
        case $v in plain) f="" ;; asan) f=$ASAN ;; tsan) f=$TSAN ;; esac
        build "swap-$v" "$S/swap" $f
        build "ok-$v" "$ROOT/compiler" $f
    done
    for v in plain asan tsan; do
        expect_fail "swap-$v-inside" "$S/swap-$v" "$N_CTL" release-inside-swap
        expect_fail "ok-$v-inside" "$S/ok-$v" "$N_CTL" release-inside-swap
    done
    for v in asan tsan; do
        expect_fail "swap-$v-beside" "$S/swap-$v" "$N_CTL" release-beside-swaps
        expect_fail "ok-$v-beside" "$S/ok-$v" "$N_CTL" release-beside-swaps
    done
    ;;
inert-control)
    copy_tree "$S/inert-tree"
    f="$S/inert-tree/src/backend/concurrent_map_test.c"
    perl -0pi -e 's/(uint64_t absent_length = counted_key\(999, absent\), last = 0;\n)/$1    if (getenv("INERT") != NULL \&\& strcmp(getenv("INERT"), "reader") == 0)\n        return NULL;\n/' "$f"
    perl -0pi -e 's/(static void \*hold_entries\(void \*arg\) \{\n)/$1    if (getenv("INERT") != NULL \&\& strcmp(getenv("INERT"), "holder") == 0)\n        return NULL;\n/' "$f"
    perl -0pi -e 's/(static void \*release_reserves\(void \*arg\) \{\n)/$1    if (getenv("INERT") != NULL \&\& strcmp(getenv("INERT"), "releaser") == 0)\n        return NULL;\n/' "$f"
    [ "$(diff "$ROOT/compiler/src/backend/concurrent_map_test.c" "$f" | grep -c '^>')" -eq 6 ] || { echo "PATCH DID NOT APPLY AS EXPECTED"; diff "$ROOT/compiler/src/backend/concurrent_map_test.c" "$f"; exit 1; }
    build inert "$S/inert-tree"
    build ok-plain "$ROOT/compiler"
    for who in reader holder releaser; do
        args=""
        [ "$who" = releaser ] && args="release-beside-swaps"
        echo "== $who inert"
        export INERT=$who
        timed "inert-$who" "$S/inert" $args || true
        unset INERT
        echo "RESULT inert-$who: $(cat "$S/inert-$who.out" | grep -c 'concurrent-map-test:') message lines"
    done
    echo "== none inert (the same build)"
    timed inert-none "$S/inert" || true
    ;;
late-control)
    # The same late start of one thread, in the sources before this work and in
    # the sources after it: the one thread begins its loop after a pause that
    # lets the other side finish its fixed quota.
    old_tree
    copy_tree "$S/late-new-tree"
    late_patch "$S/old/compiler/src/backend/concurrent_map_test.c"
    late_patch "$S/late-new-tree/src/backend/concurrent_map_test.c"
    build late-old "$S/old/compiler"
    build late-new "$S/late-new-tree"
    for who in reader holder releaser; do
        export LATE=$who
        expect_fail "late-old-$who" "$S/late-old" "$N_CTL"
        expect_fail "late-new-$who" "$S/late-new" "$N_CTL"
    done
    unset LATE
    ;;
newtest-control)
    # Each of the three tests added after the first hand-in, on the sources as
    # they are (it must pass) and with the one wrong behaviour it guards.
    build nt-ok "$ROOT/compiler"
    for arg in swap-hook release-inside-self-swap owed-bound; do
        expect_fail "nt-ok-$arg" "$S/nt-ok" "$N_CTL" "$arg"
    done
    CM=concurrent_map.c
    T=concurrent_map_test.c
    variant hook-after $CM 's/    WF_CMAP_SWAPPING\(a, b\);\n(    table \*current = [^\n]*\n[^\n]*\n[^\n]*\n    atomic_store_explicit\(&b->current, current, memory_order_relaxed\);\n)/$1    WF_CMAP_SWAPPING(a, b);\n/' \
        swap-hook "a swap's hook ran after the exchange began"
    variant hook-before $CM 's/(    wf_cmap \*first = [^\n]*\n)(    lock_map\(first\);\n    if \(b != a\)\n        lock_map\(first == a \? b : a\);\n)    WF_CMAP_SWAPPING\(a, b\);\n/$1    WF_CMAP_SWAPPING(a, b);\n$2/' \
        swap-hook "a swap's hook ran without a map's lock"
    variant hook-twice $CM 's/(    WF_CMAP_SWAPPING\(a, b\);\n)/$1$1/' \
        swap-hook "a swap did not run its hook once"
    variant hook-never $CM 's/    WF_CMAP_SWAPPING\(a, b\);\n//' \
        swap-hook "a swap did not run its hook once"
    nolocks='s/(void wf_cmap_swap.*?#undef SWAP_FIELD)/my $f = $1; $f =~ s{    lock_map\(first\);\n    if \(b != a\)\n        lock_map\(first == a \? b : a\);\n}{    (void)first;\n}; $f =~ s{    unlock_map\(a\);\n    if \(b != a\)\n        unlock_map\(b\);\n}{}; $f/se'
    variant locks-gone-self $CM "$nolocks" \
        release-inside-self-swap "a release inside a swap of a map with itself freed a reserve"
    variant locks-gone-hook $CM "$nolocks" \
        swap-hook "a swap's hook ran without a map's lock"
    variant bound-100 $T 's/at\.tv_sec - since->start\.tv_sec >= 10\)/at.tv_sec - since->start.tv_sec >= 100)/' \
        owed-bound "a wait 11 s old for an act did not end the test"
    variant no-early-return $T 's/    if \(now >= want\)\n        return 0;\n//' \
        owed-bound "a wait for an act already made asked again or started its clock"
    ;;
*)
    echo "usage: $0 calibrate|before|after|swap-control|inert-control|late-control|newtest-control" >&2
    exit 2
    ;;
esac
