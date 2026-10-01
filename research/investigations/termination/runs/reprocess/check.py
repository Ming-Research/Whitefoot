from graph import *
CL=["Chars","S_col","S_grp","S_tr","S_cell","S_table","S_other","E_html","E_tbl","E_other","Comment","Doctype","EOF"]
R={};SCC={}
for c in CL:
    g=graph(c); R[c],cyc=ranks(g); SCC[c]=cyc
# rank table
print("| mode | "+" | ".join(CL)+" |")
print("|---|"+"---|"*len(CL))
for m in MODES: print("| %s | "%m+" | ".join(str(R[c][m]) for c in CL)+" |")
# per-edge check
n=0;bad=[];sccedges=0
per={}
for i,(frm,cs,to,stk) in sorted(E.items()):
    if i==34: continue
    rows=[]
    for c in cs:
        drops=[];
        for f in frm:
            for t in to:
                n+=1
                d=R[c][f]-R[c][t]
                if d>0: drops.append(d)
                else:
                    if c=="S_table" and f in SCC[c][0] and t in SCC[c][0] and i in (38,50): sccedges+=1; drops.append(0)
                    else: bad.append((i,c,f,t,d))
        rows.append("%s:%s"%(c,"D" if 0 in drops else "R-%d..%d"%(min(drops),max(drops))))
    per[i]=rows
print("checked (edge,class,from,to) tuples (excluding E34):",n,"bad:",bad,"SCC-tuples using D:",sccedges)
for i in sorted(per): print(i,"; ".join(per[i]))
# E34 targets: nothing needed
