import json,subprocess,os,shutil,sys
S=os.path.dirname(os.path.abspath(__file__))  # holds the extracted Snowghost 09d33ba renderer/; its parent holds loops.json, the extracted loop texts
W='/home/user/snowghost/whitefoot/compiler/target/gate/whitefootc'
E={str(e['id']):e for e in json.load(open(S+'/../loops.json'))}
spec={'6':('i','up'),'13':('idx','up'),'59':('i','up'),'74':('position','up'),'83':('i','up'),'109':('pos','up'),
'113':('output^.inner.len','down'),'125':('pos','up'),'135':('hi','down'),'152':('slot','up'),'170':('position','up'),
'172':('buffer^.index','up'),'173':('buffer^.index','up'),'181':('buffer^.index','up'),'318':('pos','up'),'319':('e','down'),
'321':('pos','up'),'323':('pos','up'),'329':('pos','up'),'348':('step','up')}
only=sys.argv[1:] or sorted(spec,key=int)
res=[]
for i in only:
    expr,d=spec[i]; e=E[i]; rel=e['file']  # renderer/...
    path=os.path.join(S,rel); orig=open(path).read(); L=orig.split('\n')
    a=e['line']-1; b=e['end']-1
    h=a
    while not L[h].rstrip().endswith('{'): h+=1
    ind=' '*(len(L[a])-len(L[a].lstrip())+2)
    snap=f'{ind}let term_before_{i} = {expr};'
    inv=f'{ind}invariant term_progress_{i}: ' + (f'term_before_{i} < {expr};' if d=='up' else f'{expr} < term_before_{i};')
    L2=L[:h+1]+[snap]+L[h+1:b]+[inv]+L[b:]
    open(path,'w').write('\n'.join(L2))
    mod='pkg::'+'::'.join(os.path.dirname(rel).split('/')[1:])
    r=subprocess.run([W,'--graph','modules.wfg','--check-module',mod],cwd=S+'/renderer',capture_output=True,text=True)
    open(path,'w').write(orig)
    txt=r.stdout+r.stderr
    errs=[l for l in txt.split('\n') if 'error[' in l]
    mine=[l for l in txt.split('\n') if f'term_' in l]
    disp=[l.strip() for l in txt.split('\n') if 'disposition' in l]
    status='proved' if r.returncode==0 else ('fails at inserted invariant' if any(f'term_progress_{i}' in l for l in txt.split('\n')) else 'other error')
    res.append((i,rel,e['line'],expr,d,r.returncode,status,(errs[:1] or [''])[0][:110],(disp[:1] or [''])[0]))
    open(f'{S}/out_{i}.txt','w').write(txt)
for x in res: print('\t'.join(map(str,x)))
