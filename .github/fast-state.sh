#!/usr/bin/env bash
# The build the fast workflow keeps for each work of the wf-agents engine, on
# the 14900K, where every runner runs as one user with one home.
#
#   fast-state.sh take WORK CHECKOUT
#   fast-state.sh release
#   fast-state.sh build
#   fast-state.sh refresh CHECKOUT
#   fast-state.sh clean
#
# A work's state is the directory $HOME/fast-state/Whitefoot/<work>/, or
# .../manual/ where WORK is empty (a run a person asked for). It holds `src`,
# a clone of the repository at the commit to test, and `target`, cargo's
# target directory (CARGO_TARGET_DIR), which the `gate` profile builds
# incrementally. The tests build and run in `src` and not in the runner's own
# checkout, so a work's sources keep one path whichever runner takes its run:
# rustc's incremental cache and cargo's fingerprints are kept for the path
# they were made at. `src` moves to a new commit by git, which rewrites only
# the files that changed, so cargo sees only those as newer than its build.
#
# `take` holds the lock <dir>.lock until `release` or the end of the job (the
# runner ends a job's processes when the job ends), so two runs of one work
# at once take turns; it makes the directory on a work's first run, as a copy
# of .../main/, which `refresh` keeps at main's last commit; it moves `src` to
# the commit CHECKOUT holds, marks the directory used, and gives the job
# FAST_STATE and CARGO_TARGET_DIR. `build` builds the tests in FAST_STATE as
# the fast workflow runs them. `refresh` does the same for .../main/ under
# its lock. `clean` removes each work's directory that no run has used for 24
# hours, where no run holds it.
#
# Builds take at most CARGO_BUILD_JOBS jobs where the runner's service sets it:
# cargo reads it, and nothing here names a number of jobs.
set -euo pipefail

root="$HOME/fast-state/Whitefoot"
# A directory unused this long is removed, in minutes.
unused_minutes=1440

# The build of the tests, the same in every run and in main's, so that a
# work's first run finds main's build of every unit fresh but those it changed.
build_tests() {
    cargo test --manifest-path compiler/Cargo.toml --profile gate --lib --bins --test '*' \
        --no-run --locked --offline
}

# Moves the clone in $1/src to the commit the checkout $2 holds.
sync_source() {
    local dir=$1 checkout=$2 commit
    commit=$(git -C "$checkout" rev-parse HEAD)
    if [ ! -d "$dir/src/.git" ]; then
        rm -rf "$dir/src"
        git clone --quiet --no-checkout "$checkout" "$dir/src"
    fi
    git -C "$dir/src" fetch --quiet --no-tags "$checkout" HEAD
    test "$(git -C "$dir/src" rev-parse FETCH_HEAD)" = "$commit"
    # A forced checkout writes again each file whose times or inode differ
    # from what the index recorded, as after a copy from main or a touch, and
    # cargo would then see it as newer than its build. The refresh reads those
    # files and records them as they are where their content is the same, so
    # the checkout writes only the files the commit changes.
    git -C "$dir/src" update-index -q --refresh > /dev/null || :
    git -C "$dir/src" checkout --quiet --force --detach "$commit"
    git -C "$dir/src" clean -ffdxq
    echo "$dir/src is at $commit"
}

case ${1:-} in
take)
    [ $# -eq 3 ] || { echo "usage: fast-state.sh take WORK CHECKOUT" >&2; exit 2; }
    work=${2:-manual}
    if [ "$work" != manual ] && ! [[ $work =~ ^[0-9]+$ ]]; then
        echo "::error::The work's number is digits, not $work"
        exit 1
    fi
    dir="$root/$work"
    mkdir -p "$root"
    held="$RUNNER_TEMP/fast-state-held"
    rm -f "$held" "$RUNNER_TEMP/fast-state-release"
    echo "Waiting for $dir.lock"
    # Held by a process of its own, which outlives this step.
    nohup flock "$dir.lock" sh -c 'touch "$1"; while [ ! -e "$2" ]; do sleep 0.2; done' \
        sh "$held" "$RUNNER_TEMP/fast-state-release" > /dev/null 2>&1 &
    while [ ! -e "$held" ]; do sleep 0.2; done
    if [ ! -d "$dir/src/.git" ]; then
        rm -rf "$dir" "$dir.new"
        if [ -d "$root/main/src/.git" ]; then
            echo "Seeding $dir from $root/main"
            flock -s "$root/main.lock" cp -a --reflink=auto "$root/main" "$dir.new"
        else
            echo "$root/main holds no build yet: this run builds from nothing"
            mkdir -p "$dir.new"
        fi
        mv "$dir.new" "$dir"
    fi
    sync_source "$dir" "$3"
    touch "$dir/used"
    du -sh "$dir/target" 2>/dev/null || true
    {
        echo "FAST_STATE=$dir"
        echo "CARGO_TARGET_DIR=$dir/target"
    } >> "$GITHUB_ENV"
    ;;
release)
    touch "$RUNNER_TEMP/fast-state-release"
    ;;
build)
    cd "$FAST_STATE/src"
    build_tests
    ;;
refresh)
    [ $# -eq 2 ] || { echo "usage: fast-state.sh refresh CHECKOUT" >&2; exit 2; }
    dir="$root/main"
    mkdir -p "$dir"
    exec 9> "$root/main.lock"
    flock 9
    sync_source "$dir" "$2"
    cd "$dir/src"
    cargo fetch --locked --manifest-path compiler/Cargo.toml
    CARGO_TARGET_DIR="$dir/target" build_tests
    touch "$dir/used"
    du -sh "$dir/target"
    ;;
clean)
    mkdir -p "$root"
    for dir in "$root"/*/ "$root"/*.new/; do
        dir=${dir%/}
        [ -d "$dir" ] || continue
        name=${dir##*/}
        [ "$name" != main ] || continue
        lock="$dir.lock"
        case $name in *.new) lock="${dir%.new}.lock" ;; esac
        if [ -e "$dir/used" ] && [ -z "$(find "$dir/used" -mmin +"$unused_minutes")" ]; then
            continue
        fi
        if [ ! -e "$dir/used" ] && [ -z "$(find "$dir" -maxdepth 0 -mmin +"$unused_minutes")" ]; then
            continue
        fi
        # A run that holds it is using it.
        if flock -n "$lock" rm -rf "$dir"; then
            echo "Removed $dir, unused for 24 hours"
        else
            echo "Kept $dir: a run holds it"
        fi
    done
    df -h "$root"
    ;;
*)
    echo "usage: fast-state.sh take WORK CHECKOUT | release | build | refresh CHECKOUT | clean" >&2
    exit 2
    ;;
esac
