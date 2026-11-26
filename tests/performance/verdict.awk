# Paired baseline/candidate ratios, with the whole expected matrix of the kernels
# named by -v kernels="a b ..." (compare.sh reads them from the Makefile).
# One adverse width is a visible suspect; two in one kernel fail. CPU reports
# use the wall pair count and never select the blocking verdict.
function refuse(message) { print "REFUSED: " message; bad=1; exit 2 }
function positive(value) { return value ~ /^[0-9]+([.][0-9]*)?([eE][+-]?[0-9]+)?$/ && value+0>0 && value+0<1e100 }
BEGIN {
    FS="[ \t]+"
    nk=split(kernels, names, " ")
    if(nk<1) refuse("no kernels named")
    for(k=1;k<=nk;k++) {
        if(names[k] !~ /^[a-z][a-z0-9_]*$/ || names[k] in known) refuse("invalid or repeated kernel name " names[k])
        known[names[k]]=1
    }
    if(cpus !~ /^[0-9]+$/ || cpus+0<2) refuse("at least two available CPUs required")
    widths[1]=1; widths[2]=2; nw=2
    if(cpus+0>=4) widths[++nw]=4
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
    printf "VERDICT: %s -- %d kernel(s) adverse at two widths\n",failures ? "FAIL" : "PASS",failures
    exit failures ? 1 : 0
}
