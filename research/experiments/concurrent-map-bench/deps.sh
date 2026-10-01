#!/bin/sh
# Serves concurrent-map-bench: the pinned comparators' sources. `fetch` is the
# only network step; `build` is offline.
#
#   sh deps.sh fetch               download and verify every pinned source
#   OUT=<absolute dir> sh deps.sh build
#                                  build liburcu, oneTBB and the Rust library
#                                  into OUT and unpack Boost's headers there
set -eu
cd "$(dirname "$0")"
mode=${1:?fetch or build required}
case "$mode" in fetch|build) ;; *) echo 'deps.sh: fetch or build' >&2; exit 1;; esac
cache=${WHITEFOOT_SCRATCH_ROOT:-${TMPDIR:-/tmp}/whitefoot}/whitefoot-concurrent-map-deps
boost=boost-1.92.0-b2-nodocs.tar.xz
boost_sha=ea7b982002cc9dfbe59b0b217b206f470dc75f3de0bb2973d844118934d82411
boost_url=https://github.com/boostorg/boost/releases/download/boost-1.92.0/$boost
urcu=userspace-rcu-0.15.7.tar.bz2
urcu_sha=2556b83adc0f9b3ac8024e613e17d014d04c4c49110604ce55fcb14eae32edd3
urcu_url=https://lttng.org/files/urcu/$urcu
tbb_pin=3046c8b0c29df995980003ea24f4d78c80ec0c8d
cuckoo_pin=6a2555d551b7703d7176a5d114219a02c55d4038
sha() {
    if command -v sha256sum > /dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}
archive() {
    if test ! -e "$cache/$1"; then
        mkdir -p "$cache"
        curl -sSfL -o "$cache/$1.part" "$3"
        mv "$cache/$1.part" "$cache/$1"
    fi
    if test "$(sha "$cache/$1")" != "$2"; then
        echo "deps.sh: $cache/$1 does not match its pin; delete it and rerun fetch" >&2
        exit 1
    fi
}
clone() {
    if test ! -e "$cache/$1"; then
        mkdir -p "$cache"
        git init -q "$cache/$1"
        git -C "$cache/$1" remote add origin "$3"
        git -C "$cache/$1" fetch -q --depth=1 origin "$2"
        git -C "$cache/$1" checkout -q --detach "$2"
    fi
    if test "$(git -C "$cache/$1" rev-parse HEAD)" != "$2" ||
        test -n "$(git -C "$cache/$1" status --porcelain --untracked-files=all)"; then
        echo "deps.sh: $cache/$1 is not a clean checkout of $2; delete it and rerun fetch" >&2
        exit 1
    fi
}
if test "$mode" = fetch; then
    archive "$boost" "$boost_sha" "$boost_url"
    archive "$urcu" "$urcu_sha" "$urcu_url"
    clone onetbb "$tbb_pin" https://github.com/uxlfoundation/oneTBB.git
    clone libcuckoo "$cuckoo_pin" https://github.com/efficient/libcuckoo.git
    cargo fetch --locked --manifest-path rust/Cargo.toml
    (cd go && go mod download)
    echo "concurrent-map-bench sources PASS: boost=$boost_sha urcu=$urcu_sha oneTBB=$tbb_pin libcuckoo=$cuckoo_pin"
    exit 0
fi
: "${OUT:?absolute output directory required}"
case "$OUT" in /*) ;; *) echo 'deps.sh: OUT must be absolute' >&2; exit 1;; esac
test "$OUT" != /
for f in "$boost" "$urcu" onetbb libcuckoo; do
    test -e "$cache/$f" || { echo "deps.sh: $cache/$f is missing; run fetch first" >&2; exit 1; }
done
jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)
mkdir -p "$OUT/include" "$OUT/src"
if test ! -d "$OUT/include/boost"; then
    tar -xJf "$cache/$boost" -C "$OUT/src" boost-1.92.0/boost
    mv "$OUT/src/boost-1.92.0/boost" "$OUT/include/boost"
fi
if test ! -e "$OUT/lib/liburcu.a"; then
    rm -rf "${OUT:?}/src/userspace-rcu-0.15.7"
    tar -xjf "$cache/$urcu" -C "$OUT/src"
    (cd "$OUT/src/userspace-rcu-0.15.7" &&
        ./configure -q --prefix="$OUT" --enable-static --disable-shared > /dev/null &&
        make -s -j "$jobs" > /dev/null && make -s install > /dev/null)
fi
if test ! -e "$OUT/lib/libtbb.so" && test ! -e "$OUT/lib64/libtbb.so"; then
    cmake -S "$cache/onetbb" -B "$OUT/src/onetbb-build" -G Ninja \
        -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$OUT" -DCMAKE_INSTALL_LIBDIR=lib \
        -DTBB_TEST=OFF -DTBB_EXAMPLES=OFF -DTBB_STRICT=OFF > /dev/null
    cmake --build "$OUT/src/onetbb-build" -j "$jobs" > /dev/null
    cmake --install "$OUT/src/onetbb-build" > /dev/null
fi
rm -rf "${OUT:?}/include/libcuckoo"
cp -R "$cache/libcuckoo/libcuckoo" "$OUT/include/libcuckoo"
CARGO_TARGET_DIR="$OUT/cmaps-target" cargo build --release --locked --offline \
    --manifest-path rust/Cargo.toml
echo "concurrent-map-bench deps PASS: $OUT"
