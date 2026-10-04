#!/usr/bin/env python3
"""Compare public Halo compile output with independently normalized luac -l -l.

Python is used for binary-safe fixture transport and interpreting the PUC
listing, not to implement Lua parsing or compile the Whitefoot module.
"""
from pathlib import Path
import argparse, difflib, hashlib, json, re, shutil, subprocess, tempfile, time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
OPS = ['MOVE','LOADK','LOADBOOL','LOADNIL','GETUPVAL','GETGLOBAL','GETTABLE','SETGLOBAL','SETUPVAL','SETTABLE','NEWTABLE','SELF','ADD','SUB','MUL','DIV','MOD','POW','UNM','NOT','LEN','CONCAT','JMP','EQ','LT','LE','TEST','TESTSET','CALL','TAILCALL','RETURN','FORLOOP','FORPREP','TFORLOOP','SETLIST','CLOSE','CLOSURE','VARARG']
PAIR = {'EQ','LT','LE','TEST','TESTSET','TFORLOOP'}


def programs():
    cases = {}
    expressions = [
        'nil', 'false', 'true', '17', '"a\\000b"', '...', 'x', 't.x', 't[x]',
        '(x)', '-x', 'not x', '#t', 'x+y', 'x-y', 'x*y', 'x/y', 'x%y', 'x^y',
        'x..y..z', 'x==y', 'x~=y', 'x<y', 'x<=y', 'x>y', 'x>=y',
        'x and y', 'x or y', 'x and y or z', 'x or y and z', 'not (x<y)',
        '(x<y) and (y<z)', '(x<y) or (y<z)', '(x and y)==z', 'f(x)',
        't:m(x,y)', 'f "abc"', 'f {x,y}', '(f(x))', '{}', '{x,y,z}',
        '{x=1,[y]=2,z,3;4}', '{1,f()}', '{...}', '2+3*4', '(2+3)^2',
        '2^3^2', '-2^2', '(-2)^2', '1/0', '0/0', '1%0', '(-1)^0.5',
        '1e308*1e308', '1e308-1e308', '0*-1', '5%(-2)', '-0',
        '1<2', '1==2', '"x"=="y"', 'true~=false', 'nil==nil',
    ]
    for op in ('+','-','*','/','%','^','==','~=','<','<=','>','>='):
        # RR is covered above; force RK, KR and KK without numeral folding.
        expressions.extend([f'x {op} 1', f'1 {op} x', f'true {op} false'])
    for i, e in enumerate(expressions):
        cases[f'expression-{i:03}-return'] = f'return {e}'
        cases[f'expression-{i:03}-value'] = f'local r = {e}; return r'
        cases[f'expression-{i:03}-condition'] = f'if {e} then return 1 else return 2 end'
        cases[f'expression-{i:03}-closure'] = f'local x,y,z,t,f; return function(...) return {e} end'
    statements = {
        'assignment-swap': 'local a,b=1,2; a,b=b,a; return a,b',
        'assignment-conflict-key': 'local t,i={},1; t[i],i=3,4; return t,i',
        'assignment-conflict-table': 'local t,i={},1; t[i],t=3,{}; return t',
        'assignment-surplus': 'local a,b=1,2,3,4; a=5,6,7; return a,b',
        'assignment-short': 'local a,b,c=1; a,b,c=f(); return a,b,c',
        'assignment-zero-return': 'a,b,c,d=1,2,f()',
        'local-shadow': 'local x=1; do local x=x+1; local y; x=y end; return x',
        'local-recursion': 'local function f(n) if n<1 then return 1 end return n*f(n-1) end return f(7)',
        'global-function': 'function f(a,b) return a+b end return f(1,2)',
        'dotted-function': 'function a.b.c(x) return x end',
        'method-function': 'function a.b:c(x,...) return self,x,... end',
        'capture-local': 'local x=1; return function() x=x+1; return x end',
        'capture-upvalue': 'local x=1; return function() return function() x=2; return x end end',
        'capture-repeated': 'local x=1; return function() return x,x,x end',
        'capture-close': 'do local x=1; f=function() return x end end return f()',
        'while-break': 'local x=0; while x<10 do x=x+1; if x==4 then break end end return x',
        'while-upvalue-break': 'while x do local y=1; f=function() return y end; break end',
        'repeat-local': 'repeat local x=f(); local y=x+1 until y>4',
        'repeat-upvalue': 'repeat local x=f(); g=function() return x end until x>4',
        'repeat-break': 'repeat local x=1; if f() then break end until x==2',
        'numeric-for': 'local x=0; for i=1,10 do x=x+i end return x',
        'numeric-for-step': 'for i=10,1,-2 do f(i) end',
        'numeric-for-capture': 'for i=1,10 do local x=i; f=function() return i,x end end',
        'generic-for': 'for k,v in pairs(t) do f(k,v) end',
        'generic-for-short': 'for k in f, t, nil do if k then break end end',
        'generic-for-many': 'for a,b,c,d in f() do f(a,b,c,d) end',
        'if-elseif': 'if a then f() elseif b and c then g() elseif d or e then h() else i() end',
        'return-open-call': 'return 1,2,f()',
        'return-open-vararg': 'return 1,2,...',
        'return-tail-method': 'return t:m(1,2)',
        'return-parenthesized-call': 'return (f())',
        'call-open-arguments': 'f(1,2,g())',
        'call-open-varargs': 'f(1,2,...)',
        'constructor-batches': 'return {' + ','.join(str(i) for i in range(1,154)) + '}',
        'constructor-hash-mixed': 'return {a=1,2,b=3,4,[f()]=g(),h()}',
        'numbers-over-256': 'local t={' + ','.join(str(i) for i in range(300)) + '}; return t[299]+300',
        'strings-over-256': 'local t={' + ','.join(f'"s{i}"' for i in range(300)) + '}; return t["s299"]',
        'globals-over-256': ';'.join(f'g{i}={i}' for i in range(257)),
        'globals-after-many-constants': 'local t={' + ','.join(str(i) for i in range(300)) + '}; return g',
        'setlist-extension': 'return {' + ','.join(['1'] * 25601) + '}',
        'local-and': 'local a,b; return a and b',
        'local-or': 'local a,b; return a or b',
        'local-and-value': 'local a,b; local c=a and b; return c',
        'local-or-value': 'local a,b; local c=a or b; return c',
        'local-short-condition': 'local a,b,c; if a and (b or c) then return b end',
        'local-comparison-value': 'local a,b,c; return (a<b) and b or c',
        'legacy-arg': 'return function(...) return arg end',
        'vararg-fixed-params': 'return function(a,b,...) return a,b,... end',
    }
    cases.update(statements)
    return {k: v.encode() for k, v in cases.items()}


def malformed():
    cases = [
        'local = 1', 'local x x', 'local x, = 1', 'local function (x) end',
        'local function f(x x) end', 'function f(1) end', 'function f(x,) end',
        'function f(...,x) end', 'function f() return ... end', 'function f.x:() end',
        'function f:x end', 'function f()\nreturn 1', 'function f() return 1',
        'do\nreturn 1', 'do return 1', 'if x then\nreturn 1', 'if x then return 1',
        'if x f() end', 'if then end', 'if x then elseif then end',
        'while x do\nf()', 'while x f() end', 'repeat\nf()', 'repeat f() until',
        'for =1,2 do end', 'for i 1,2 do end', 'for i=1 do end', 'for i=1,2 end',
        'for i in do end', 'for i,j=1,2 do end', 'for i=1,2 do\nf()',
        'return x+', 'return (x', 'return (\nx', 'return x[1', 'return x[\n1',
        'return x.', 'return x:', 'return x:m', 'return x:m[1]', 'return f(1,)',
        'return f(\n1', 'return f\n(1)', 'return {x=}', 'return {x 1}',
        'return {[x]=}', 'return {1\n', 'return {x=1 y=2}', 'return "unfinished',
        'return [=[unfinished', 'return 1e+', 'return 1..2', 'return @',
        'break', 'do break end', 'return 1 return 2', ';', 'x', '1=2',
        '(x)=1', 'f()=1', 'a,b()=1,2', 'local x = end', 'return ~x',
        'local ' + ','.join('x'+str(i) for i in range(201)),
        'return f(' + ','.join('x' for _ in range(260)) + ')',
        'return ' + '(' * 210 + 'x' + ')' * 210,
        'function f()\nlocal ' + ','.join('x'+str(i) for i in range(201)) + '\nend',
        '--[[ unfinished', 'return "\\256"', 'return [=x',
        'do local x end\n'*32768,
        'local '+','.join('x'+str(i) for i in range(61))+'; return function() return '+','.join('x'+str(i) for i in range(61))+' end',
        ','.join('g'+str(i) for i in range(202))+'=1',
        'while x do\n'+ 'x=1;\n'*65538+'end',
        'while x do local t={' + ','.join(['1']*128000) + '} end',
    ]
    return {f'error-{i:03}': s.encode() for i, s in enumerate(cases)}


def lua_string(s):
    assert s.startswith('"') and s.endswith('"'), s
    body=s[1:-1]; out=bytearray(); i=0
    escapes={'a':7,'b':8,'f':12,'n':10,'r':13,'t':9,'v':11}
    while i<len(body):
        if body[i]!='\\': out.append(ord(body[i]));i+=1;continue
        i+=1;c=body[i]
        if c.isdigit():
            m=re.match(r'\d{1,3}',body[i:]);out.append(int(m[0]));i+=len(m[0])
        else:out.append(escapes.get(c,ord(c)));i+=1
    return bytes(out)


def parse_listing(data):
    protos=[];current=None;mode=''
    for line in data.decode('latin1').splitlines():
        m=re.match(r'^(?:main|function) <.*> \((\d+) instructions?, .* at (\S+)\)',line)
        if m:
            current={'address':m[2],'ncode':int(m[1]),'code':[],'ks':[]};protos.append(current);mode='code';continue
        m=re.match(r'^(\d+)(\+?) params?, (\d+) slots?, (\d+) upvalues?, (\d+) locals?, (\d+) constants?, (\d+) functions?',line)
        if m:
            current.update(params=int(m[1]),vararg=int(bool(m[2])),stack=int(m[3]),nups=int(m[4]),nk=int(m[6]),children=int(m[7]));continue
        if line.startswith('constants ('):mode='constants';continue
        if line.startswith(('locals (','upvalues (')):mode='debug';continue
        if mode=='code':
            m=re.match(r'^\s+(\d+)\s+\[(\d+|-)\]\s+(\w+)\s+([^;]*)(?:;\s*(.*))?$',line)
            if m:
                args=[int(x) for x in m[4].strip().split()];current['code'].append([int(m[1])-1,int(m[2]) if m[2]!='-' else 0,m[3],args,m[5] or ''])
        elif mode=='constants':
            m=re.match(r'^\s+\d+\s+(.*)$',line)
            if m:
                s=m[1]
                if s=='nil':v=['0']
                elif s=='false':v=['1']
                elif s=='true':v=['2']
                elif s.startswith('"'):v=['4',lua_string(s).hex() or '-']
                else:v=['3',s]
                current['ks'].append(v)
    if not protos:raise ValueError('luac listing has no prototypes')
    for p in protos:
        if len(p['ks'])!=p['nk']:raise ValueError('constant listing truncated')
        if not p['code'] or p['code'][-1][2]!='RETURN':raise ValueError('missing final RETURN')
    return protos


def normalized(protos):
    ids={p['address']:i for i,p in enumerate(protos)};lines=[];codebase=0;kbase=0
    for pid,p in enumerate(protos):
        code=p['code'];mapping={};cells=0;skipped=set()
        for pos,ln,op,args,comment in code:
            if pos in skipped: mapping[pos]=codebase+cells-1;continue
            mapping[pos]=codebase+cells;cells+=1
            if op in PAIR:
                skipped.add(pos+1)
                if not any(q[0]==pos+1 and q[2]=='JMP' for q in code):raise ValueError('unpaired conditional')
            if op=='SETLIST' and args[2]==0:mapping[pos+1]=codebase+cells-1
        mapping[p['ncode']]=codebase+cells
        lines.append(['P',str(pid),str(codebase),str(kbase),str(p['nk']),str(p['params']),str(p['stack']),str(p['nups']),str(p['vararg'])])
        for k,v in enumerate(p['ks']):lines.append(['K',str(kbase+k)]+v)
        for pos,ln,op,args,comment in code:
            if pos in skipped:continue
            a=args[0] if args else 0;dest=None
            if op in PAIR:
                j=next(q for q in code if q[0]==pos+1)
                dest=mapping[pos+2+j[3][0]]
            elif op in ('JMP','FORLOOP','FORPREP'):dest=mapping[pos+1+args[-1]]
            out=[]
            if op=='LOADK':
                k=-1-args[1];name='LoadK' if k<256 else 'LoadKx';out=[a,k]
            elif op in ('GETGLOBAL','SETGLOBAL'):name='GetGlobal' if op=='GETGLOBAL' else 'SetGlobal';out=[a,-1-args[1]]
            elif op in ('GETTABLE','SELF'):
                c=args[2];name=('GetTable' if op=='GETTABLE' else 'Self')+('K' if c<0 else 'R');out=[a,args[1],-1-c if c<0 else c]
            elif op in ('ADD','SUB','MUL','DIV','MOD','POW','SETTABLE','EQ','LT','LE'):
                stem={'ADD':'Add','SUB':'Sub','MUL':'Mul','DIV':'Div','MOD':'Mod','POW':'Pow','SETTABLE':'SetTable','EQ':'EqJmp','LT':'LtJmp','LE':'LeJmp'}[op]
                b,c=args[1:];name=stem+('K' if b<0 else 'R')+('K' if c<0 else 'R');out=[a,-1-b if b<0 else b,-1-c if c<0 else c]
                if dest is not None:out.append(dest)
            elif op=='JMP':name='Jmp';out=[dest]
            elif op in ('FORLOOP','FORPREP'):name='ForLoop' if op=='FORLOOP' else 'ForPrep';out=[a,dest]
            elif op=='TFORLOOP':name='TForLoop';out=[a,args[1],dest]
            elif op=='TEST':name='TestJmp';out=[a,args[2],dest]
            elif op=='TESTSET':name='TestSetJmp';out=args+[dest]
            elif op=='CLOSURE':name='Closure';out=[a,ids[comment]]
            elif op=='SETLIST':name='SetList';out=[a,args[1],(255-args[2] if args[2]<0 else args[2]) or int(comment)]
            else:
                name={'MOVE':'Move','LOADBOOL':'LoadBool','LOADNIL':'LoadNil','GETUPVAL':'GetUpval','SETUPVAL':'SetUpval','NEWTABLE':'NewTable','UNM':'Unm','NOT':'Not','LEN':'Len','CONCAT':'Concat','CALL':'Call','TAILCALL':'TailCall','RETURN':'Return','CLOSE':'Close','VARARG':'VarArg'}[op];out=args
            lines.append(['C',str(mapping[pos]),str(ln),name]+[str(x) for x in out])
        codebase+=cells;kbase+=p['nk']
    for k in range(256):lines.append(['K',str(kbase+k),'0'])
    # Halo prints each array separately, in index order.
    lines.sort(key=lambda x:({'P':0,'K':1,'C':2}[x[0]],int(x[1])))
    return lines


def parse_halo(data):
    return [s.rstrip('\t').split('\t') for s in data.decode().splitlines()]


def digest():
    h=hashlib.sha256()
    for p in sorted((ROOT/'lib/halo/compile').glob('*'))+ [ROOT/'lib/halo/value/module.wfm']:
        if p.is_file():h.update(p.name.encode()+b'\0'+p.read_bytes())
    return h.hexdigest()


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--lua-source',type=Path,help='Redis Lua C source directory; required unless --luac is supplied')
    ap.add_argument('--halo-dump',type=Path,required=True)
    ap.add_argument('--luac',type=Path,help='Use an already-built scratch oracle; otherwise build PUC from sources')
    ap.add_argument('--sample',type=int)
    ap.add_argument('--filter')
    ap.add_argument('--output',type=Path)
    a=ap.parse_args()
    if a.luac is None and a.lua_source is None:ap.error("--lua-source is required unless --luac is supplied")
    if a.sample is not None and a.sample <= 0:ap.error('--sample must be positive')
    cases=[(str(p.relative_to(ROOT)),p.read_bytes(),False) for p in sorted((HERE.parent/'halo-oracle/scripts').rglob('*.lua'))]
    if not cases:raise RuntimeError('oracle corpus missing')
    cases += [(k,v,False) for k,v in programs().items()]
    cases += [(k,v,True) for k,v in malformed().items()]
    if a.filter:cases=[x for x in cases if a.filter in x[0]]
    if a.sample:cases=cases[:a.sample]
    if not cases:raise RuntimeError('no selected cases')
    d=digest();report={'implementation_sha256':d,'driver_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'files_compared':0,'valid_programs':0,'matched_valid_programs':0,'matched_error_programs':0,'malformed_programs':0,'cells_compared':0,'expected_cells':0,'cells_matching':0,'mismatches':[],'case_seconds':[],'cases':[],'cell_variants':{}}
    # The controls change actual candidate output and must be rejected.
    controls=0
    with tempfile.TemporaryDirectory(prefix='halo-compile-oracle-') as tmp:
        luac = a.luac
        if luac is None:
            build=Path(tmp)/'puc';build.mkdir()
            for src in a.lua_source.iterdir():
                if src.suffix in ('.c','.h'):shutil.copy2(src,build/src.name)
            units=['lapi','lcode','ldebug','ldo','ldump','lfunc','lgc','llex','lmem','lobject','lopcodes','lparser','lstate','lstring','ltable','ltm','lundump','lvm','lzio','strbuf','fpconv','lauxlib','luac','print']
            t=time.monotonic();subprocess.run(['cc','-O2','-c','lcode.c','-o','lcode.o'],cwd=build,check=True,timeout=30)
            print(f'PUC C build sample (lcode): {time.monotonic()-t:.3f}s',flush=True)
            t=time.monotonic();subprocess.run(['cc','-O2','-o','luac','lcode.o']+[x+'.c' for x in units if x!='lcode']+['-lm'],cwd=build,check=True,timeout=60)
            report['puc_build_seconds']=round(time.monotonic()-t,6);luac=build/'luac'
        for name,source,mal in cases:
            start=time.monotonic()
            oracle=subprocess.run([str(luac),'-l','-l','-o',tmp+'/chunk.luac','-'],input=source,capture_output=True,timeout=30)
            halo=subprocess.run([str(a.halo_dump)],input=source,capture_output=True,timeout=30)
            if halo.returncode:raise RuntimeError(f'{name}: Halo dump exit {halo.returncode}: {halo.stderr.decode(errors="replace")}')
            got=parse_halo(halo.stdout)
            if oracle.returncode:
                if not mal:raise RuntimeError(f'{name}: valid fixture rejected by PUC: {oracle.stderr!r}')
                msg=oracle.stderr.split(b': ',1)[1].rstrip(b'\n')
                m=re.match(rb'stdin:(\d+): ',msg)
                if m:expected=[['E',m[1].decode(),msg.hex()]]
                else:
                    # luaM_growaux_ emits raw text without a source location.
                    expected=[['E','-',msg.hex()]]
                    if got and got[0][0]=='E':got[0][1]='-'
                report['malformed_programs']+=1
                if got==expected and controls==9:
                    damaged=[x[:] for x in got];damaged[0][-1]='00';assert damaged!=expected;controls+=1
            else:
                if mal:raise RuntimeError(f'{name}: malformed fixture accepted by PUC')
                expected=normalized(parse_listing(oracle.stdout));report['valid_programs']+=1
                report['expected_cells']+=sum(x[0]=='C' for x in expected)
                if controls==0 and got==expected:
                    for record,column in [('P',2),('P',6),('K',2),('C',2),('C',3),('C',-1)]:
                        damaged=[x[:] for x in got];idx=next(i for i,x in enumerate(damaged) if x[0]==record)
                        damaged[idx][column]='MUTATED';assert damaged!=expected;controls+=1
                    for record in ('P','K','C'):
                        damaged=[x[:] for x in got];idx=next(i for i,x in enumerate(damaged) if x[0]==record)
                        del damaged[idx];assert damaged!=expected;controls+=1
            report['files_compared']+=1
            ecells=[x for x in expected if x[0]=='C'];gcells=[x for x in got if x[0]=='C']
            report['cells_compared']+=min(len(ecells),len(gcells))
            report['cells_matching']+=sum(x==y for x,y in zip(ecells,gcells))
            for cell in gcells:
                report['cell_variants'][cell[3]]=report['cell_variants'].get(cell[3],0)+1
            if got==expected:
                report['matched_error_programs' if mal else 'matched_valid_programs']+=1
            if got!=expected:
                diff=''.join(difflib.unified_diff(['\t'.join(x)+'\n' for x in expected],['\t'.join(x)+'\n' for x in got],fromfile='PUC',tofile='Halo'))
                report['mismatches'].append({'case':name,'diff':diff})
                print('MISMATCH',name, diff[:2500],flush=True)
            report['case_seconds'].append(round(time.monotonic()-start,6))
            report['cases'].append({'name':name,'source_sha256':hashlib.sha256(source).hexdigest(),'oracle_sha256':hashlib.sha256(oracle.stdout+oracle.stderr).hexdigest(),'matched':got==expected})
    memory_cases = [
        ('string-token', b'return "x"', ['exhaust']),
        ('identifier-token', b'local x', ['exhaust']),
        ('synthetic-arg-local', b'return function(...) end', ['exhaust']),
        ('lookahead-name', b'return {a b}', ['exhaust', 'after-one']),
        ('lookahead-string', b'return {a "b"}', ['exhaust', 'after-one']),
        ('synthetic-for-local', b'for i=1,2 do end', ['exhaust', 'after-one']),
    ]
    report['intern_exhaustion_checks'] = []
    for name, source, arguments in memory_cases:
        candidate = subprocess.run([str(a.halo_dump)] + arguments, input=source, capture_output=True, timeout=30)
        if candidate.returncode:raise RuntimeError(f'{name}: dump exit {candidate.returncode}')
        expected = [['E', '1', b'not enough memory'.hex()]]
        actual = parse_halo(candidate.stdout)
        if actual != expected:raise AssertionError(f'{name}: expected {expected}, got {actual}')
        report['intern_exhaustion_checks'].append(name)
    assert digest()==d,'implementation changed during comparison' 
    report['mutation_controls_detected']=controls
    if a.output:a.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k not in ('mismatches','case_seconds','cases')},indent=2))
    print('mismatches:',len(report['mismatches']))
    return bool(report['mismatches'])


if __name__=='__main__':raise SystemExit(main())
