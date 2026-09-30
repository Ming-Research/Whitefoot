from graph import *
def analyse(drop):
    U={}
    for i,(frm,cs,to,stk) in E.items():
        if i in drop: continue
        for f in frm:
            for t in to:
                for c in cs:
                    U.setdefault((f,t),{}).setdefault(c,[]).append(i)
    adj={m:sorted({t for (f,t) in U if f==m}) for m in MODES}
    cycles=[]
    def dfs(start,v,path,seen):
        for w in adj[v]:
            if w==start: cycles.append(path[:])
            elif w not in seen and MODES.index(w)>MODES.index(start):
                seen.add(w);path.append(w);dfs(start,w,path,seen);path.pop();seen.discard(w)
    for s in MODES: dfs(s,s,[s],{s})
    out=[]
    for cy in cycles:
        cls=set(ALLC)
        for a,b in zip(cy,cy[1:]+cy[:1]): cls&=set(U[(a,b)].keys())
        out.append((cy,cls,U))
    return out
r=analyse({34,38})
print("mode cycles (excluding reset edges 34,38):",len(r))
for cy,cls,U in r:
    ids=[]
    for a,b in zip(cy,cy[1:]+cy[:1]):
        ids.append((a,b,sorted({i for c in U[(a,b)] for i in U[(a,b)][c]})))
    print(" ","->".join(cy+[cy[0]]),"| alive for classes:",sorted(cls) or "none")
    for a,b,i in ids: print("      ",a,"->",b,"edges",i,"classes",sorted(U[(a,b)]))
r=analyse({34})
alive=[(cy,cls) for cy,cls,U in r if cls]
print("cycles with 38 (S_table) alive:")
for cy,cls in alive: print(" ","->".join(cy+[cy[0]]),sorted(cls))
