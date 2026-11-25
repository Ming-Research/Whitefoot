# Paired baseline/candidate ratios, with the whole expected kernel/width matrix.
# One adverse width is a visible suspect; two in one kernel fail. CPU reports
# use the wall pair count and never select the blocking verdict. A kernel in
# -v aside (the data-placement control's aside.txt) is not judged: its rows are
# shown and count as no failure, suspect or CPU report.
function refuse(message) { print "REFUSED: " message; bad=1; exit 2 }
function positive(value) { return value ~ /^[0-9]+([.][0-9]*)?([eE][+-]?[0-9]+)?$/ && value+0>0 && value+0<1e100 }
BEGIN {
    FS="[ \t]+"
    nk=split("mandelbrot records fir quadrature stencil", names, " ")
    for(k=1;k<=nk;k++) known[names[k]]=1
    if(cpus !~ /^[0-9]+$/ || cpus+0<2) refuse("at least two available CPUs required")
    widths[1]=1; widths[2]=2; nw=2
    if(cpus+0>=4) widths[++nw]=4
    na=split(aside, listed, " ")
    for(a=1;a<=na;a++) {
        if(!(listed[a] in known)) refuse("unknown kernel set aside: " listed[a])
        if(listed[a] in set_aside) refuse("kernel set aside twice: " listed[a])
        set_aside[listed[a]]=1
    }
}
/^#/ || NF==0 { next }
{
    if(NF!=6 || !($1 in known) || $2 !~ /^[0-9]+$/ ||
       !positive($3) || $4 !~ /^[0-5]$/ || $5!="5" || !positive($6))
        refuse("invalid or underpowered paired row " NR)
    if(($2!=1 && $2!=2 && $2!=4) || $2+0>cpus+0) { skipped++;next }
    id=$1 SUBSEP $2
    if(id in wall) refuse("duplicate kernel/width " $1 " W=" $2)
    wall[id]=$3+0; lower[id]=$4+0; cpu[id]=$6+0
    adverse[id]=($3+0<0.97 && $4+0>=4)
    if(adverse[id]) adverse_count[$1]++
}
END {
    if(bad) exit 2
    for(k=1;k<=nk;k++) for(w=1;w<=nw;w++)
        if(!((names[k],widths[w]) in wall)) refuse("missing " names[k] " W=" widths[w])
    print "rule: wall < 0.97 with >=4/5 lower pairs; two distinct adverse widths fail a kernel."
    print "A single width is a suspect. CPU < 0.90 uses wall-pair counts and is report-only."
    print "This rule can miss a real regression confined to one width; it does not prove a suspect is noise."
    for(k=1;k<=nk;k++) {
        kernel=names[k]
        if(kernel in set_aside) {
            not_judged++
            for(w=1;w<=nw;w++) {
                id=kernel SUBSEP widths[w]
                printf "%-12s W=%d wall=%.6f lower=%d/5 cpu=%.6f not judged (data placement)\n",kernel,widths[w],wall[id],lower[id],cpu[id]
            }
            printf "not judged: %s (data placement)\n",kernel
            continue
        }
        if(adverse_count[kernel]>=2) failures++
        for(w=1;w<=nw;w++) {
            id=kernel SUBSEP widths[w]
            result=adverse[id] ? (adverse_count[kernel]>=2 ? "FAIL" : "suspect") : "pass"
            printf "%-12s W=%d wall=%.6f lower=%d/5 cpu=%.6f %s\n",kernel,widths[w],wall[id],lower[id],cpu[id],result
            if(result=="suspect") { suspects++;printf "suspect: %s W=%d\n",kernel,widths[w] }
            if(cpu[id]<0.90 && lower[id]>=4) { reports++;printf "cpu report: %s W=%d\n",kernel,widths[w] }
        }
    }
    printf "skipped %d unrecorded/oversubscribed rows; %d suspect(s); %d cpu report(s)\n",skipped,suspects,reports
    printf "VERDICT: %s -- %d kernel(s) adverse at two widths",failures ? "FAIL" : "PASS",failures
    if(not_judged) printf "; %d kernel(s) not judged (data placement)",not_judged
    printf "\n"
    exit failures ? 1 : 0
}
