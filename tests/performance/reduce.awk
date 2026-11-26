# Complete two-arm matrix of the kernels named by -v kernels="a b ..." (compare.sh
# reads them from the Makefile's KERNELS). Sample zero is the unrecorded warmup;
# each process median uses samples 1..5, then arms are paired by pass 0..4.
function refuse(message) { print "REFUSED: " message > "/dev/stderr"; bad=1; exit 2 }
function median(a, n, i,j,x) {
    for (i=2;i<=n;i++) { x=a[i];j=i-1;while(j>0 && a[j]>x){a[j+1]=a[j];j--}a[j+1]=x }
    return a[3]
}
BEGIN {
    FS="[ \t]+"; OFS="\t"
    nk=split(kernels, names, " ")
    if(nk<1) refuse("no kernels named")
    for(k=1;k<=nk;k++) {
        if(names[k] !~ /^[a-z][a-z0-9_]*$/ || names[k] in known) refuse("invalid or repeated kernel name " names[k])
        known[names[k]]=1
    }
    if(cpus !~ /^[0-9]+$/ || cpus+0<2) refuse("at least two available CPUs required")
    widths[1]=1; widths[2]=2; nw=2
    if(cpus+0>=4) widths[++nw]=4
    for(w=1;w<=nw;w++) required_width[widths[w]]=1
}
/^#/ || NF==0 { next }
{
    if(NF!=8 || !($1 in known) || ($2!="baseline" && $2!="candidate") ||
       !($3 in required_width) || $4 !~ /^[0-4]$/ || $5 !~ /^[0-5]$/ ||
       $6 !~ /^[0-9]+$/ || $6+0<=0 || $7 !~ /^[0-9]+$/ || $7+0<=0 ||
       $8 !~ /^[0-9]+$/ || $8+0<=0) refuse("invalid raw row " NR)
    id=$1 SUBSEP $2 SUBSEP $3 SUBSEP $4 SUBSEP $5
    if(id in wall) refuse("duplicate raw row " NR)
    wall[id]=$6+0; cpu[id]=$7+0
    if($1 in compared && compared[$1]!=$8+0) refuse("inconsistent output extent for " $1)
    compared[$1]=$8+0
}
END {
    if(bad) exit 2
    for(k=1;k<=nk;k++) for(w=1;w<=nw;w++) {
        kernel=names[k]; width=widths[w]; lower=0
        for(p=0;p<5;p++) {
            for(a=0;a<2;a++) {
                arm=a ? "candidate" : "baseline"
                for(s=0;s<=5;s++) {
                    id=kernel SUBSEP arm SUBSEP width SUBSEP p SUBSEP s
                    if(!(id in wall)) refuse("missing " kernel " " arm " W=" width " pass=" p " sample=" s)
                    if(s) { times[s]=wall[id]; cores[s]=cpu[id] }
                }
                process_wall[a]=median(times,5); process_cpu[a]=median(cores,5)
            }
            ratios[p+1]=process_wall[0]/process_wall[1]
            cpu_ratios[p+1]=process_cpu[0]/process_cpu[1]
            if(process_wall[0]<process_wall[1]) lower++
        }
        # Preserve precision for decisions; only the verdict's display rounds.
        printf "%s\t%d\t%.17g\t%d\t5\t%.17g\n", kernel,width,median(ratios,5),lower,median(cpu_ratios,5)
    }
}
