import itertools, sys
MODES = ["Initial","BeforeHtml","BeforeHead","InHead","InHeadNoscript","AfterHead","InBody","Text","InTable","InTableText","InCaption","InColumnGroup","InTableBody","InRow","InCell","InTemplate","AfterBody","InFrameset","AfterFrameset","AfterAfterBody","AfterAfterFrameset"]
RESET = ["InCell","InRow","InTableBody","InCaption","InColumnGroup","InTable","InTemplate","InHead","InBody","InFrameset","BeforeHead","AfterHead"]
ALLC = ["Chars","S_col","S_grp","S_tr","S_cell","S_table","S_other","E_html","E_tbl","E_other","Comment","Doctype","EOF"]
S_ALL = ["S_col","S_grp","S_tr","S_cell","S_table","S_other"]
E_ALL = ["E_html","E_tbl","E_other"]
# id: (from list, classes, to list, stack effect)  stack: '+k' push, '-k' pop (>=1), '0'
E = {}
def e(i, frm, cls, to, stk="0"): E[i]=(frm,cls,to,stk)
e(1,["Initial"],["Chars"],["BeforeHtml"])
e(2,["Initial"],S_ALL,["BeforeHtml"])
e(3,["Initial"],E_ALL,["BeforeHtml"])
e(4,["Initial"],["EOF"],["BeforeHtml"])
e(5,["BeforeHtml"],S_ALL,["BeforeHead"],"+1")
e(6,["BeforeHtml"],["E_html","E_other"],["BeforeHead"],"+1")
e(7,["BeforeHtml"],["EOF"],["BeforeHead"],"+1")
e(8,["BeforeHead"],S_ALL,["InHead"],"+1")
e(9,["BeforeHead"],["E_html","E_other"],["InHead"],"+1")
e(10,["BeforeHead"],["EOF"],["InHead"],"+1")
e(11,["InHead"],S_ALL,["AfterHead"],"-1")
e(12,["InHead"],["E_html","E_other"],["AfterHead"],"-1")
e(13,["InHead"],["EOF"],["AfterHead"],"-1")
e(14,["InHeadNoscript"],S_ALL,["InHead"],"-1")
e(15,["InHeadNoscript"],["E_other"],["InHead"],"-1")
e(16,["InHeadNoscript"],["Chars"],["InHead"],"-1")
e(17,["InHeadNoscript"],["EOF"],["InHead"],"-1")
e(18,["Text"],["EOF"],[m for m in MODES if m!="Text"],"-1")
e(19,["AfterHead"],S_ALL,["InBody"],"+1")
e(20,["AfterHead"],["E_html","E_other"],["InBody"],"+1")
e(21,["AfterHead"],["EOF"],["InBody"],"+1")
e(22,["AfterBody"],["Chars"],["InBody"])
e(23,["AfterBody"],S_ALL,["InBody"])
e(24,["AfterBody"],["E_tbl","E_other"],["InBody"])
e(25,["AfterAfterBody"],S_ALL,["InBody"])
e(26,["AfterAfterBody"],["Chars"],["InBody"])
e(27,["AfterAfterBody"],E_ALL,["InBody"])
e(28,["InBody"],["E_html"],["AfterBody"])
e(29,["InTemplate"],["S_grp"],["InTable"],"0;Tpl top replaced")
e(30,["InTemplate"],["S_col"],["InColumnGroup"],"0;Tpl top replaced")
e(31,["InTemplate"],["S_tr"],["InTableBody"],"0;Tpl top replaced")
e(32,["InTemplate"],["S_cell"],["InRow"],"0;Tpl top replaced")
e(33,["InTemplate"],["S_other","S_table"],["InBody"],"0;Tpl top replaced")
e(34,["InTemplate","InBody","InTable","InTableBody","InRow","InCell","InCaption","InColumnGroup"],["EOF"],RESET,"-k>=1;Tpl-1")
e(35,["InTable","InTableBody","InRow"],["Chars"],["InTableText"])
e(36,["InTable"],["S_col"],["InColumnGroup"],"+1(-k)")
e(37,["InTable"],["S_tr","S_cell"],["InTableBody"],"+1(-k)")
e(38,["InTable","InTableBody","InRow"],["S_table"],RESET,"-k>=1")
e(39,["InTableText"],S_ALL+E_ALL+["Comment","Doctype","EOF"],["InTable","InTableBody","InRow"],"+j")
e(40,["InTableBody"],["S_cell"],["InRow"],"+1(-k)")
e(41,["InTableBody"],["S_grp","S_col"],["InTable"],"-k>=2")
e(42,["InTableBody"],["E_tbl"],["InTable"],"-k>=2")
e(43,["InRow"],["S_grp","S_col","S_tr"],["InTableBody"],"-k>=1")
e(44,["InRow"],["E_tbl"],["InTableBody"],"-k>=1")
e(45,["InRow"],["E_tbl"],["InTableBody"],"-k>=1")
e(46,["InCell"],["E_tbl"],["InRow"],"-k>=1")
e(47,["InCell"],["S_grp","S_col","S_cell","S_tr"],["InRow"],"-k>=1")
e(48,["InCaption"],["E_tbl"],["InTable"],"-k>=1")
e(49,["InCaption"],["S_grp","S_col","S_cell","S_tr"],["InTable"],"-k>=1")
e(50,["InColumnGroup"],["S_grp","S_tr","S_cell","S_table","S_other"],["InTable"],"-1")
e(51,["InColumnGroup"],E_ALL,["InTable"],"-1")

def graph(cls, drop=(34,)):
    g={m:set() for m in MODES}
    for i,(frm,cs,to,stk) in E.items():
        if i in drop: continue
        if cls in cs:
            for f in frm:
                for t in to: g[f].add((t,i))
    return g
def sccs(g):
    idx={};low={};st=[];on=set();res=[];c=[0]
    sys.setrecursionlimit(10000)
    def sc(v):
        idx[v]=low[v]=c[0];c[0]+=1;st.append(v);on.add(v)
        for w,_ in g[v]:
            if w not in idx: sc(w);low[v]=min(low[v],low[w])
            elif w in on: low[v]=min(low[v],idx[w])
        if low[v]==idx[v]:
            comp=[]
            while True:
                w=st.pop();on.discard(w);comp.append(w)
                if w==v:break
            res.append(comp)
    for v in MODES:
        if v not in idx: sc(v)
    return res
def ranks(g):
    comps=sccs(g); cid={}
    for k,cm in enumerate(comps):
        for m in cm: cid[m]=k
    memo={}
    def R(k):
        if k in memo:return memo[k]
        best=0
        for m in comps[k]:
            for t,_ in g[m]:
                if cid[t]!=k: best=max(best,R(cid[t])+1)
        memo[k]=best;return best
    return {m:R(cid[m]) for m in MODES}, [cm for cm in comps if len(cm)>1 or any(t==cm[0] for t,_ in g[cm[0]])]
if __name__=="__main__":
    for cls in ALLC:
        g=graph(cls); r,cyc=ranks(g)
        print("==",cls,"cycles:",cyc)
        byr={}
        for m in MODES: byr.setdefault(r[m],[]).append(m)
        for k in sorted(byr,reverse=True): print("  R=%d: %s"%(k,", ".join(byr[k])))
