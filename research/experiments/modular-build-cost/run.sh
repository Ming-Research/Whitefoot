#!/usr/bin/env bash
# Measures module-program build cost and runtime across whitefootc's cache and
# link-fragment modes. Explicitly requested research, never a gate:
#
#   WHITEFOOTC=compiler/target/gate/whitefootc \
#     research/experiments/modular-build-cost/run.sh
#
# Every program is copied into a scratch directory first, so edits never touch
# the repository. Output is tab-separated: workload, mode, step, wall
# milliseconds, and the build report the compiler printed.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repository=$(cd "$here/../../.." && pwd)
compiler=${WHITEFOOTC:-$repository/compiler/target/gate/whitefootc}
compiler=$(cd "$(dirname "$compiler")" && pwd)/$(basename "$compiler")
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

milliseconds() { date +%s%N | awk '{ printf "%d", $1 / 1000000 }'; }

# One timed invocation: prints workload, mode, step, wall time and the last
# report line.
measure() {
    local workload=$1 mode=$2 step=$3
    shift 3
    local start end report
    start=$(milliseconds)
    report=$("$compiler" "$@" 2>&1 | tail -n 1) || true
    end=$(milliseconds)
    printf '%s\t%s\t%s\t%d\t%s\n' "$workload" "$mode" "$step" $((end - start)) "$report"
}

# Build cost of one entry: cold, warm, then after a body edit the caller
# supplies as a sed expression on one file.
builds() {
    local workload=$1 source=$2 entry=$3 edited=$4 expression=$5
    for mode in image module function; do
        local tree=$scratch/$workload-$mode cache=$scratch/$workload-$mode.cache
        rm -rf "$tree" "$cache"
        cp -R "$source" "$tree"
        local fragments=()
        [ "$mode" = image ] || fragments=(--fragments "$mode")
        cd "$tree"
        measure "$workload" "$mode" cold --graph modules.wfg --entry "$entry" \
            -o "$scratch/$workload-$mode.bin" --cache "$cache" "${fragments[@]}" --report
        measure "$workload" "$mode" warm --graph modules.wfg --entry "$entry" \
            -o "$scratch/$workload-$mode.bin" --cache "$cache" "${fragments[@]}" --report
        sed -i "$expression" "$edited"
        measure "$workload" "$mode" body-edit --graph modules.wfg --entry "$entry" \
            -o "$scratch/$workload-$mode.bin" --cache "$cache" "${fragments[@]}" --report
        cd "$here"
    done
    local tree=$scratch/$workload-plain
    rm -rf "$tree"
    cp -R "$source" "$tree"
    cd "$tree"
    measure "$workload" none cold --graph modules.wfg --entry "$entry" \
        -o "$scratch/$workload-none.bin"
    cd "$here"
}

# Module checks: cold without a cache, cold and warm with one, then after an
# implementation edit and after an interface edit.
checks() {
    local workload=$1 source=$2 body=$3 body_expression=$4 interface=$5 interface_expression=$6
    local tree=$scratch/$workload-check cache=$scratch/$workload-check.cache
    rm -rf "$tree" "$cache"
    cp -R "$source" "$tree"
    cd "$tree"
    measure "$workload" check none --graph modules.wfg --check-modules
    measure "$workload" check cold --graph modules.wfg --check-modules --cache "$cache"
    measure "$workload" check warm --graph modules.wfg --check-modules --cache "$cache"
    sed -i "$body_expression" "$body"
    recheck "$workload" body-edit "$cache"
    sed -i "$interface_expression" "$interface"
    recheck "$workload" interface-edit "$cache"
    cd "$here"
}

# One cached check of every module and entry after an edit: its wall time and
# how many of the verdicts it recomputed rather than reused.
recheck() {
    local workload=$1 step=$2 cache=$3 start end lines
    start=$(milliseconds)
    lines=$("$compiler" --graph modules.wfg --check-modules --cache "$cache" --report 2>/dev/null || true)
    end=$(milliseconds)
    printf '%s\tcheck\t%s\t%d\trecomputed %s of %s\n' "$workload" "$step" $((end - start)) \
        "$(grep -c '"reused":false' <<<"$lines" || true)" "$(grep -c '"reused"' <<<"$lines" || true)"
}

# A controlled scaling program: a chain of modules, each publishing
# `count` functions that call their predecessor module's function of the
# same index, and a root entry that calls the last module.
generate() {
    local root=$1 modules=$2 count=$3
    rm -rf "$root"
    mkdir -p "$root"
    local graph=$root/modules.wfg previous=""
    : > "$graph"
    for ((m = 0; m < modules; m++)); do
        local name=m$m directory=$root/m$m
        mkdir -p "$directory"
        if [ -z "$previous" ]; then
            printf 'pkg::%s: [];\n' "$name" >> "$graph"
        else
            printf 'pkg::%s: [pkg::%s];\n' "$name" "$previous" >> "$graph"
        fi
        : > "$directory/module.wfm"
        : > "$directory/body.wf"
        for ((f = 0; f < count; f++)); do
            printf 'public fn f%d(value: u64) -> result: u64 pure doc "Step %d of module %d.";\n' \
                "$f" "$f" "$m" >> "$directory/module.wfm"
            [ "$f" -eq $((count - 1)) ] || printf '\n' >> "$directory/module.wfm"
            {
                printf 'fn f%d(value: u64) -> result: u64 pure {\n' "$f"
                if [ -z "$previous" ]; then
                    printf '  let result = value *wrap %d_u64;\n' $((f + 3))
                else
                    printf '  let inner = pkg::%s::f%d(value: value);\n' "$previous" "$f"
                    printf '  let result = inner +wrap %d_u64;\n' $((m + f))
                fi
                printf '  return result;\n}\n'
                [ "$f" -eq $((count - 1)) ] || printf '\n'
            } >> "$directory/body.wf"
        done
        previous=$name
    done
    printf 'pkg: [pkg::%s, std::process];\n\nentry app = pkg::main;\n' "$previous" >> "$graph"
    printf 'public fn main() -> status: std::process::ExitStatus pure doc "Runs the chain.";\n' > "$root/module.wfm"
    {
        printf 'fn main() -> status: std::process::ExitStatus pure {\n'
        printf '  let total = pkg::%s::f0(value: 1_u64);\n' "$previous"
        printf '  let low = iand(total, 1_u64);\n'
        printf '  match cvt.checked::<u64, u8>(low) {\n'
        printf '    Ok(value: code) => {\n      return std::process::exit_status(code: code);\n    }\n'
        printf '    Err(error: refused) => {\n      return std::process::exit_status(code: 255_u8);\n    }\n'
        printf '  }\n}\n'
    } > "$root/main.wf"
}

printf 'workload\tmode\tstep\twall_ms\treport\n'

demo=$repository/research/investigations/modular-compilation/demo
checks demo "$demo" runtime/report.wf 's/second.payload;/second.payload;\n  let spare = digest +wrap 0_u8;/' \
    runtime/queue/module.wfm 's/Creates an empty queue./Creates an empty queue of capacity jobs./'
builds demo "$demo" inspect runtime/report.wf 's/second.payload;/second.payload;\n  let spare = digest +wrap 0_u8;/'

for modules in 8 32; do
    generated=$scratch/chain-$modules
    generate "$generated" "$modules" 16
    middle=m$((modules / 2))
    checks "chain-$modules" "$generated" "$middle/body.wf" 's/+wrap \([0-9]*\)_u64;/+wrap 99_u64;/' \
        m0/module.wfm 's/Step 0 of module 0./Step zero of module zero./'
    builds "chain-$modules" "$generated" app "$middle/body.wf" 's/+wrap \([0-9]*\)_u64;/+wrap 99_u64;/'
done

# A hot path that crosses many fragments: a chain of `depth` modules whose
# `step` mixes the result of the previous module's, called once per
# iteration of the root module's loop, so every step is a call into another
# module's fragment unless the link imports and inlines it.
generate_crossing() {
    local root=$1 depth=$2
    rm -rf "$root"
    mkdir -p "$root"
    local graph=$root/modules.wfg previous=""
    : > "$graph"
    for ((m = 0; m < depth; m++)); do
        local name=s$m directory=$root/s$m
        mkdir -p "$directory"
        if [ -z "$previous" ]; then
            printf 'pkg::%s: [];\n' "$name" >> "$graph"
        else
            printf 'pkg::%s: [pkg::%s];\n' "$name" "$previous" >> "$graph"
        fi
        printf 'public fn step(state: u64) -> result: u64 pure doc "Step %d of the crossing chain.";\n' \
            "$m" > "$directory/module.wfm"
        {
            printf 'fn step(state: u64) -> result: u64 pure {\n'
            if [ -z "$previous" ]; then
                printf '  let shifted = ishl.wrap(state, 13_u32);\n'
                printf '  let result = ixor(state, shifted);\n'
            else
                printf '  let inner = pkg::%s::step(state: state);\n' "$previous"
                printf '  let shifted = ishr(inner, %d_u32);\n' $((m % 7 + 1))
                printf '  let result = ixor(inner, shifted);\n'
            fi
            printf '  return result;\n}\n'
        } > "$directory/step.wf"
        previous=$name
    done
    printf 'pkg: [pkg::%s, std::process];\n\nentry bench = pkg::main;\n' "$previous" >> "$graph"
    printf 'public fn main() -> status: std::process::ExitStatus pure doc "Runs the crossing loop and exits with the low bits of its state.";\n' \
        > "$root/module.wfm"
    {
        printf 'fn main() -> status: std::process::ExitStatus pure {\n'
        printf '  let state = 88172645463325252_u64;\n'
        printf '  for @spin (index in 0_u64..100000000_u64) {\n'
        printf '    let next = pkg::%s::step(state: state);\n' "$previous"
        printf '    set state = next;\n  }\n'
        printf '  let low = iand(state, 127_u64);\n'
        printf '  match cvt.checked::<u64, u8>(low) {\n'
        printf '    Ok(value: code) => {\n      return std::process::exit_status(code: code);\n    }\n'
        printf '    Err(error: refused) => {\n      return std::process::exit_status(code: 255_u8);\n    }\n'
        printf '  }\n}\n'
    } > "$root/main.wf"
}

# Runtime quality: each benchmark built in every mode, five runs each. `full`
# is the full link-time optimization comparator, which optimizes the program
# and its runtime units as one region.
crossing=$scratch/crossing-source
generate_crossing "$crossing" 8
for workload in rng crossing; do
    case $workload in
        rng) source=$here/rng ;;
        crossing) source=$crossing ;;
    esac
    for mode in none image module function full; do
        tree=$scratch/$workload-$mode
        rm -rf "$tree"
        cp -R "$source" "$tree"
        cd "$tree"
        arguments=(--graph modules.wfg --entry bench -o "$scratch/$workload-$mode.bin")
        case $mode in
            none) ;;
            full) arguments+=(--full-lto) ;;
            *) arguments+=(--cache "$scratch/$workload-$mode.cache") ;;
        esac
        case $mode in module | function) arguments+=(--fragments "$mode") ;; esac
        "$compiler" "${arguments[@]}"
        cd "$here"
        for run in 1 2 3 4 5; do
            start=$(milliseconds)
            "$scratch/$workload-$mode.bin" || true
            end=$(milliseconds)
            printf '%s\t%s\trun-%d\t%d\t\n' "$workload" "$mode" "$run" $((end - start))
        done
    done
done
