#!/bin/bash
# Summarizes downloaded compute-regression artifacts, one directory per run id
# (`gh run download <id> -n compute-regression-ubuntu-24.04 -D <dir>/<id>`),
# as one line per run: CPU class, the null/comparison/slow/placement verdicts,
# and which kernels' linked images (exe_same) or emitted module objects
# (o_same) are byte-identical between the two arms. Usage: summarize-artifacts.sh DIR
cd "$1" || exit 2
for d in *; do
  r=$d/performance-results; [ -d "$r" ] || continue
  a=$d/performance-baseline; b=$d/performance-candidate
  cpu=$(grep -m1 "Model name" "$r/identity.txt" | sed 's/.*: *//; s/ [0-9]*-Core.*//; s/AMD EPYC /E/; s/INTEL(R) XEON(R) //; s/Intel(R) Xeon(R) //')
  same=""; sameo=""
  for k in mandelbrot records fir quadrature stencil; do
    [ -f "$a/$k" ] && cmp -s "$a/$k" "$b/$k" && same="$same$k,"
    [ -f "$a/$k.o" ] && cmp -s "$a/$k.o" "$b/$k.o" && sameo="$sameo$k,"
  done
  v() { f=$r/$1/verdict.txt; if [ -f "$f" ]; then grep -h "^VERDICT" "$f" | sed 's/VERDICT: //; s/ -- .*//'; else echo -; fi; }
  echo "$d|$cpu|null=$(v null)|cmp=$(v comparison)|slow=$(v slow)|plc=$(v placement)|exe_same=$same|o_same=$sameo"
done
