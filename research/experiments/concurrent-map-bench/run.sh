#!/bin/sh
# Serves concurrent-map-bench: runs one profile of the measurement over every
# implementation and writes rows.csv, one row per cell and per check, with
# the repetition in the first column.
#
#   sh run.sh verify|quick|full BUILD_DIR OUT_DIR
#
# CMAP_IMPLS overrides the implementations (driver prefixes) and CMAP_CPUS
# the placement list, the CPUs threads are pinned to in order.
set -eu
profile=${1:?profile}
build=${2:?build directory}
out=${3:?output directory}
cpus=${CMAP_CPUS:-$(seq -s, 0 $(($(getconf _NPROCESSORS_ONLN) - 1)))}
ncpu=$(echo "$cpus" | tr ',' '\n' | wc -l | tr -d ' ')
impls=${CMAP_IMPLS:-"wf_current empty mutex_flat flat boost_cfm tbb_chm libcuckoo urcu_lfht growt phmap papaya dashmap scc stdmap java_chm go_syncmap go_xsync dotnet_cd"}
all="uniform:read,mostly-read,balanced,update,churn,grow zipf:read,mostly-read,balanced,update one:update"
case "$profile" in
verify) sizes=1024 reps=1 warm=20 dur=50 threads="1,$ncpu" plan=$all ;;
quick)
    sizes=1048576 reps=3 warm=200 dur=500 threads="1,$ncpu"
    plan="uniform:mostly-read,balanced zipf:mostly-read,balanced one:update"
    ;;
full)
    sizes="1024 1048576 16777216" reps=3 warm=200 dur=1000 plan=$all
    threads=1 t=2
    while test "$t" -lt "$ncpu"; do threads="$threads,$t"; t=$((t * 2)); done
    test "$ncpu" -gt 1 && threads="$threads,$ncpu"
    ;;
*) echo "run.sh: unknown profile $profile" >&2; exit 2 ;;
esac
# The managed drivers do not pin their threads; the process is confined to
# the placement list instead.
bench() {
    impl=$1
    shift
    case "$impl" in
    java_chm) taskset -c "$cpus" java -Xms6g -Xmx6g -XX:+AlwaysPreTouch -cp "$build/java" Bench "$@" ;;
    go_syncmap) taskset -c "$cpus" "$build/bench-go" --impl syncmap "$@" ;;
    go_xsync) taskset -c "$cpus" "$build/bench-go" --impl xsync "$@" ;;
    dotnet_cd) DOTNET_gcServer=1 taskset -c "$cpus" dotnet "$build/dotnet/Bench.dll" "$@" ;;
    wf_current) bench_wf "$size" "$dist" "$mixes" "$warm" "$dur" "$zipf" ;;
    *) "$build/bench-$impl" "$@" ;;
    esac
}
# The wf-current control runs one thread count per process, on as many
# drivers as threads, and prints bare numbers (wf/current.wf, main); this
# turns them into the driver's rows. It has no grow cell.
bench_wf() {
    size=$1 dist=$2 mixes=$3 warm=$4 dur=$5 zipf=$6
    mask=0
    for m in $(echo "$mixes" | tr ',' ' '); do
        case $m in
        read) mask=$((mask | 1)) ;; mostly-read) mask=$((mask | 2)) ;;
        balanced) mask=$((mask | 4)) ;; update) mask=$((mask | 8)) ;;
        churn) mask=$((mask | 16)) ;;
        esac
    done
    case $dist in uniform) code=0 ;; zipf) code=1 ;; *) code=2; mask=$((mask & 8)) ;; esac
    test "$code" -eq 0 || mask=$((mask & 15))
    test "$mask" -ne 0 || return 0
    wf_status=0
    for t in $(echo "$threads" | tr ',' ' '); do
        list=$(echo "$cpus" | cut -d, -f1-"$t")
        (cd "$out" && WF_DRIVERS=$t taskset -c "$list" "$build/wf-current" \
            "$size" "$code" "$mask" "$t" "$warm" "$dur" "$(basename "$zipf")") > "$out/wf.out" || wf_status=$?
        awk -v size="$size" -v dist="$dist" -v list="$(echo "$list" | tr ',' '+')" '
            BEGIN { split("read mostly-read balanced update churn", names, " ") }
            $1 <= 4 {
                check = $1 == 4 ? "see-check-churn" : ($5 + $6 == 0 ? "pass" : "fail:misses=" ($5 + $6))
                printf "wf-current,-,%s,%s,%s,%d,%s,%.3f,%d,%.4f,%s\n", size, dist, names[$1 + 1], $2, list,
                    $3 / ($4 / 1e3), $3, $4 / 1e9, check
            }
            $1 == 9 {
                check = $2 != 0 ? "fail:missing=" $2 : ($3 != $4 ? "fail:sum=" $3 ",expected=" $4 : "pass")
                printf "wf-current,-,%s,%s,check-values,0,-,0,0,0,%s\n", size, dist, check
            }
            $1 == 8 {
                check = $2 != $3 ? "fail:live=" $2 ",expected=" $3 : "pass"
                printf "wf-current,-,%s,%s,check-churn,0,-,0,0,0,%s\n", size, dist, check
            }' "$out/wf.out"
    done
    rm -f "$out/wf.out"
    return "$wf_status"
}
mkdir -p "$out"
rows=$out/rows.csv
echo "rep,impl,flags,size,dist,mix,threads,cpus,mops,ops,seconds,check" > "$rows"
count=$(echo $impls | wc -w | tr -d ' ')
failed=0
for size in $sizes; do
    zipf=$out/zipf-$size.bin
    test -e "$zipf" || "$build/zipfgen" "$size" "$ncpu" "$zipf"
    rep=1
    while test "$rep" -le "$reps"; do
        # Each repetition starts one implementation later, so that drift
        # over a run does not fall on the same implementation each time.
        set -- $impls
        skip=$(((rep - 1) % count))
        order=
        i=0
        for impl in "$@"; do
            if test "$i" -lt "$skip"; then late="${late:-} $impl"; else order="$order $impl"; fi
            i=$((i + 1))
        done
        order="$order ${late:-}"
        late=
        for impl in $order; do
            for item in $plan; do
                dist=${item%%:*}
                mixes=${item#*:}
                part=$out/part.csv
                status=0
                bench "$impl" --size "$size" --dist "$dist" --threads "$threads" \
                    --mixes "$mixes" --warmup-ms "$warm" --duration-ms "$dur" \
                    --cpus "$cpus" --zipf "$zipf" > "$part" || status=$?
                if test "$status" -ne 0; then
                    echo "run.sh: bench-$impl exited $status at size $size, $dist" >&2
                    failed=1
                fi
                sed "s/^/$rep,/" "$part" >> "$rows"
            done
        done
        rep=$((rep + 1))
    done
done
rm -f "$out/part.csv"
fails=$(awk -F, 'NR > 1 && $12 ~ /^fail/' "$rows")
if test -n "$fails"; then
    echo "$fails" >&2
    failed=1
fi
echo "run.sh: $profile rows in $rows"
exit "$failed"
