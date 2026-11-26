# One judged round of a kernel set: the null control's paired table, then the
# comparison's, each as reduce.awk writes it and verdict.awk reads it. Run as
#   awk -v cpus=N -v kernels="a b ..." -f decide.awk NULL_PAIRED COMPARISON_PAIRED
# A width is adverse in a table when its wall ratio is below 0.97 and the
# baseline is faster in at least four of five pairs, the rule verdict.awk
# applies. For each kernel the comparison is adverse at two widths (the
# regression verdict) or it is not (`pass`). A regression verdict stands as
# `FAIL` only when the same kernel's null control, which times the one image
# against itself, shows no adverse width; otherwise the host's noise on this
# kernel reaches the threshold and the kernel is `indeterminate`, not a verdict.
# Output: one `decide: KERNEL pass|FAIL|indeterminate comparison=N null=M` line
# per kernel; exit status 0 always unless the tables are refused (2).
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
FNR==1 { table++ }
/^#/ || NF==0 { next }
{
    if(table>2) refuse("more than two tables")
    if(NF!=6 || !($1 in known) || $2 !~ /^[0-9]+$/ || !positive($3) || $4 !~ /^[0-5]$/ || $5!="5" || !positive($6))
        refuse("invalid or underpowered paired row " FNR " of table " table)
    if(($2!=1 && $2!=2 && $2!=4) || $2+0>cpus+0) next
    id=$1 SUBSEP $2
    if((table SUBSEP id) in seen) refuse("duplicate kernel/width " $1 " W=" $2 " in table " table)
    seen[table SUBSEP id]=1
    if($3+0<0.97 && $4+0>=4) adverse[table SUBSEP $1]++
}
END {
    if(bad) exit 2
    if(table!=2) refuse("two tables expected: the null control's, then the comparison's")
    for(t=1;t<=2;t++) for(k=1;k<=nk;k++) for(w=1;w<=nw;w++)
        if(!((t SUBSEP names[k] SUBSEP widths[w]) in seen)) refuse("missing " names[k] " W=" widths[w] " in table " t)
    for(k=1;k<=nk;k++) {
        c=adverse[2 SUBSEP names[k]]+0; n=adverse[1 SUBSEP names[k]]+0
        result=(c>=2) ? (n>0 ? "indeterminate" : "FAIL") : "pass"
        printf "decide: %s %s comparison=%d null=%d\n", names[k], result, c, n
    }
}
