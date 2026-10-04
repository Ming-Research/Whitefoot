#!/usr/bin/env python3
"""Compare Halo with a scratch Redis 7.0.15 Lua interpreter, entirely offline."""
import argparse
import hashlib
import importlib.util
import json
import shutil
import subprocess
import tempfile
import time
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
spec = importlib.util.spec_from_file_location('e2e', HERE.parent / 'halo-e2e/run.py')
e2e = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e2e)


def corpus():
    rows = []
    def add(group, source):
        rows.append((f'{group}/{sum(n.startswith(group+"/") for n,_ in rows):03d}', source))
    for x in ('nil','false','true','0','-0','1','-1','1.25','1e-5','1e14','1e100','math.huge','-math.huge','0/0',"''",r"'a/b\n\t\000'",'{}','{1,2,3}','{true,false,cjson.null}','{[3]=1}','{[11]=1}','{[100]=1}',"{a=1}",'{[0]=1}','{[-1]=1}','{[1.5]=1}','{[true]=1}','function() end','cjson.null'):
        add('cjson', 'return cjson.encode('+x+')')
    for text in ('null','true','false','0','-0','1.25','1e400','1e-400','01','0x10','NaN','Infinity','-Infinity','+1','1.','1e','1e+','{}','[]','[null,false,true]','{"a":null}','{"a":1,"a":2}','', ' ', '[', '{', '[1,]', '{"a":}', '{"a" 1}', '{1:2}', '[1 2]', 'true false', 'nul', '"abc', r'"\q"',r'"\uD800"',r'"\uDC00"',r'"\uD834\uDD1E"',r'"\u0000"'):
        add('cjson', 'return cjson.decode('+json.dumps(text)+')')
    for fn in ('encode','decode','encode_sparse_array','encode_max_depth','decode_max_depth','encode_number_precision','encode_keep_buffer','encode_invalid_numbers','decode_invalid_numbers','new'):
        for args in ('','nil','true','false','0','1','14','15',"'on'","'off'","'null'",'{},1'):
            add('cjson', 'return cjson.'+fn+'('+args+')')
    for x in ('math.huge','-math.huge','0/0','1.234567890123456','{}','{[100]=1}'):
        for option in ('true','false',"'null'"):
            add('cjson', 'cjson.encode_invalid_numbers('+option+'); return cjson.encode('+x+')')
    for prec in (1,2,6,13,14):
        add('cjson',f'cjson.encode_number_precision({prec}); return cjson.encode({{1.23456789012345,1e-5,1e13,-0}})')
    add('cjson','local j=cjson.new(); j.encode_number_precision(2); return {j.encode(1.2345),cjson.encode(1.2345),type(j.null),tostring(j.null)}')
    add('cjson','cjson.encode_sparse_array(true); return cjson.encode({[100]=1})')
    add('cjson','cjson.encode_sparse_array(false,0); return cjson.encode({[11]=1})')
    add('cjson','cjson.encode_max_depth(2); return cjson.encode({{{}}})')
    add('cjson','cjson.decode_max_depth(2); return cjson.decode("[[[]]]")')
    add('cjson',r'return cjson.encode("\255\128\000")')
    add('cjson',r'return cjson.decode("\"\255\128\001\"")')
    add('cjson','return {type(cjson.null),tostring(cjson.null),cjson.decode("null")==cjson.null,cjson._NAME,cjson._VERSION}')
    for fn in ('tobit','bnot','band','bor','bxor','lshift','rshift','arshift','rol','ror','bswap','tohex'):
        for args in ('','nil','true',"'123'",'0','-1','4294967295','1.5','2.5','-1.5','1,0','1,1','-1,31','1,32','1,-1','1,4,8','1,-4','1,nil'):
            add('bit', 'return bit.'+fn+'('+args+')')
    for x in ('nil','false','true','0','-1','127','128','255','256','65535','65536','-32','-33','-128','-129','-32768','-32769','2147483647','2147483648','4294967295','4294967296','9223372036854775808','1.5','1.1','math.huge','-math.huge','0/0',"''",r"'a\000\255'",'string.rep("x",31)','string.rep("x",32)','string.rep("x",256)','{}','{1,2,3}','{[3]=1}',"{a=1}",'function() end','cjson.null'):
        add('cmsgpack','return cmsgpack.pack('+x+')')
        add('cmsgpack','return cmsgpack.unpack(cmsgpack.pack('+x+'))')
    for fn in ('pack','unpack','unpack_one','unpack_limit'):
        for args in ('','nil','true','0',"''",r"'\193'",r"'\217\003ab'",r"'\196\001x'",r"'\001\002\003',2",r"'\001\002\003',1,1",r"'\001',-1",r"'\001',1,-1",r"'\001',1,10"):
            add('cmsgpack','return {'+'cmsgpack.'+fn+'('+args+')}')
    add('cmsgpack','local t={};t[1]=t; return cmsgpack.pack(t)')
    for fmt in ('b','B','h','H','l','L','T','i','I','i1','I2','i3','I8','i16','I32','i0','i33','f','d','x','c','c0','c3','s','>i2','<i2','!8bi4','!3i','!0','!','z','q','9',' ','\t','i2147483648'):
        add('struct','return struct.size('+json.dumps(fmt)+')')
        val = "'abc'" if fmt.startswith(('c','s')) else '123.5'
        add('struct','return struct.pack('+json.dumps(fmt)+','+val+')')
        add('struct','return {struct.unpack('+json.dumps(fmt)+',string.rep("\\255",40))}')
    for fn in ('pack','unpack','size'):
        for args in ('','nil','true','0',"'', ''","'b'","'b',nil","'b',true","'b','1'","'b','',0","'b','',-1","'b','',2"):
            add('struct','return {struct.'+fn+'('+args+')}')
    add('struct', 'return {struct.unpack("Bc0",struct.pack("Bc0",3,"abc"))}')
    add('struct', 'return {struct.unpack("s", "abc") }')
    for source in (
        'return {pcall(cjson.encode)}',
        'return {pcall(cjson.decode, false)}',
        'local f=cjson.decode; return f(false)',
        'local f=bit.tobit; return f(false)',
        'return cjson.null()',
        'return cjson.null+1',
        'return cjson.null[1]',
        'return cjson.null.."x"',
        'return #cjson.null',
        'return cjson.encode({[math.huge]=1})',
        'cjson.encode_invalid_numbers("null"); return cjson.encode({[math.huge]=1})',
        'cjson.encode_invalid_numbers(true); return cjson.encode({[math.huge]=1})',
        r'return cjson.decode([["\u0041\uD800"]])',
        r'return cjson.decode([["\uD834\uDD1E"]])',
        r'return cjson.decode([["\u0041\u0042"]])',
        'local j=cjson.new(); local k=j.new(); j.encode_invalid_numbers(true); return {j.encode(math.huge),k.encode_invalid_numbers(),cjson.encode_invalid_numbers()}',
        'return {cjson.encode_sparse_array()}',
        'return {cjson.encode_sparse_array("on",3,5)}',
        'cjson.encode_keep_buffer(false); return {cjson.encode_keep_buffer(),cjson.encode({1}),cjson.encode({2})}',
    ): add('cjson',source)
    for tag in (196,197,198,199,200,201,212,213,214,215,216,193):
        add('cmsgpack',f'return cmsgpack.unpack(string.char({tag}))')
    add('cmsgpack','return cmsgpack.unpack(string.char(207)..string.rep(string.char(255),8))')
    add('cmsgpack','return cmsgpack.unpack(string.char(129,192,1))')
    add('cmsgpack','return cmsgpack.unpack(string.char(129,203,127,248,0,0,0,0,0,0,1))')
    for source in ('return {struct.unpack("", "", -1)}','return {struct.unpack("c0", "abc")}', 'return struct.pack("c4","abc")', 'return {struct.unpack("!8bd",struct.pack("!8bd",1,1.25))}'):
        add('struct',source)
    add('cmsgpack', 'local t={cmsgpack.unpack(string.rep(string.char(1),300))}; return {#t,t[1],t[256],t[300]}')
    add('struct', 'local t={struct.unpack(string.rep("b",300),string.rep(string.char(1),300))}; return {#t,t[1],t[256],t[300],t[301]}')
    for lib in ('cmsgpack', 'struct'):
        expr = 'cmsgpack.pack("a\\000\\255",65535,-33)' if lib=='cmsgpack' else 'struct.pack(">I2s",65535,"a\\000\\255")'
        add(lib, 'local s='+expr+'; local t={}; for i=1,#s do t[i]=string.format("%02x",s:byte(i)) end return table.concat(t)')
    for text in ('inf','INF','-nan','nan(1)','nan(foo)','0x1.fp+3','0x1p','0x','1.e2','1\x00true','"x\x00y"','\v1','\f1','[1\x00]'):
        for invalid in ('true','false'):
            add('cjson','cjson.decode_invalid_numbers('+invalid+'); return cjson.decode('+json.dumps(text)+')')
    for width in ('9','-9','32','-32','2147483648','-2147483648','4294967295','math.huge','0/0'):
        add('bit','return bit.tohex(305419896,'+width+')')
    for n in (7996,7997,7998,7999,8000,9000):
        add('cmsgpack',f'local t={{cmsgpack.unpack(string.rep(string.char(1),{n}))}}; return #t')
        add('struct',f'local t={{struct.unpack(string.rep("b",{n}),string.rep(string.char(1),{n}))}}; return #t')
    add('cmsgpack', 'return {cmsgpack.unpack_limit(string.rep(string.char(1),8000),7997,1)}')
    add('struct', 'local t={struct.unpack(string.rep("b",7997),string.rep(string.char(1),7997),1)}; return #t')
    for fmt in ('b','B','>i3','<i8','>I8','i16','I32','f','d'):
        for val in ('-1','-123.5','255','256','4294967296','9223372036854775808','18446744073709551616','math.huge','-math.huge','0/0'):
            add('struct',f'return struct.pack("{fmt}",{val})')
    for n in (4000,4001):
        add('cmsgpack',f'local t={{}}; for i=1,{n} do t[i]=1 end return #cmsgpack.pack(unpack(t))')
    for case in ('cmsgpack-binary','struct-integers','struct-strings-floats'):
        source=(HERE.parent/'halo-oracle/scripts/libs'/(case+'.lua')).read_text()
        lines=[]
        replaced=False
        for line in source.splitlines():
            if line.startswith('local hex=string.gsub('):
                assert not replaced
                variable='packed' if case=='cmsgpack-binary' else 'p'
                lines.append('local parts={}; for i=1,#'+variable+' do parts[i]=string.format("%02x",string.byte('+variable+',i)) end; local hex=table.concat(parts)')
                replaced=True
            else:
                lines.append(line)
        assert replaced
        add('cmsgpack' if case=='cmsgpack-binary' else 'struct','\n'.join(lines))
    return rows

REFERENCE = r"""
local f=assert(io.open(arg[1], 'rb'));local s=f:read('*a');f:close()
local function quote(s)
 local t={'"'};for i=1,#s do local b=s:byte(i)
 if b==34 or b==92 then t[#t+1]='\\'..string.char(b)
 elseif b<32 or b>126 then t[#t+1]=string.format('\\u%04x',b)
 else t[#t+1]=string.char(b) end end;t[#t+1]='"';return table.concat(t)
end
local function reply(v,depth)
 local k=type(v)
 if k=='nil' or v==false or k=='function' or k=='userdata' then return '{"type":"nil","kind":"bulk"}' end
 if v==true then return '{"type":"integer","value":1}' end
 if k=='number' then
  local n=v<0 and math.ceil(v) or math.floor(v)
  if n~=n or n>=9223372036854775808 or n< -9223372036854775808 then n=-9223372036854775808 end
  return '{"type":"integer","value":'..string.format('%.0f',n)..'}'
 end
 if k=='string' then return '{"type":"bulk","bytes":'..quote(v)..'}' end
 if depth>=128 then return '{"type":"error","bytes":"ERR reply nesting too deep"}' end
 local t={};local i=1;while v[i]~=nil do t[#t+1]=reply(v[i],depth+1);i=i+1 end
 return '{"type":"array","items":['..table.concat(t,',')..']}'
end
local f,e=loadstring(s,'@user_script');local ok,v
if f then ok,v=pcall(f) else ok,v=false,e end
if ok then io.write(reply(v,0),'\n') else
 v=tostring(v):gsub('[\r\n]',' '):match('^[^%z]*');io.write('{"type":"error","bytes":'..quote('ERR '..v)..'}\n') end
"""


def build_reference(source, scratch):
    target = scratch / 'redis/deps/lua/src'
    (scratch / 'redis/src').mkdir(parents=True)
    shutil.copy2(source / 'src/solarisfixes.h', scratch / 'redis/src/solarisfixes.h')
    shutil.copytree(source / 'deps/lua/src', target, ignore=shutil.ignore_patterns('*.o','*.a','lua','luac'))
    p=target/'linit.c'; s=p.read_text()
    s=s.replace('static const luaL_Reg lualibs[]', 'int luaopen_cjson(lua_State *);\nint luaopen_cmsgpack(lua_State *);\nint luaopen_bit(lua_State *);\nint luaopen_struct(lua_State *);\n\nstatic const luaL_Reg lualibs[]')
    s=s.replace('  {NULL, NULL}', '  {"cjson", luaopen_cjson},\n  {"cmsgpack", luaopen_cmsgpack},\n  {"bit", luaopen_bit},\n  {"struct", luaopen_struct},\n  {NULL, NULL}')
    p.write_text(s)
    # luaopen_cjson returns its table without setting a global in Redis's build.
    s=p.read_text().replace('    lua_call(L, 1, 0);','    lua_call(L, 1, 1);\n    if (lib->name[0]) lua_setglobal(L, lib->name); else lua_pop(L, 1);')
    p.write_text(s)
    start=time.monotonic()
    build=subprocess.run(['make','generic','LUA_T=lua'],cwd=target,capture_output=True)
    if build.returncode: raise RuntimeError((build.stdout+build.stderr).decode())
    probe=subprocess.run([str(target/'lua'),'-e','assert(cjson and cmsgpack and bit and struct); print(cjson._VERSION,cmsgpack._VERSION)'],capture_output=True,check=True)
    return target/'lua',time.monotonic()-start,probe.stdout.decode().strip()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--compiler',required=True);ap.add_argument('--redis-source',type=Path,required=True);ap.add_argument('--binary',type=Path);ap.add_argument('--filter',default='');ap.add_argument('--report',type=Path,default=HERE/'RESULTS.md');ap.add_argument('--actual',type=Path)
    args=ap.parse_args();e2e.sensitivity()
    rows=[(n,s) for n,s in corpus() if n.startswith(args.filter)]
    if not rows: ap.error('filter matched no snippets')
    counts=Counter();failures=[]
    revision=subprocess.run(['git','rev-parse','HEAD'],cwd=ROOT,capture_output=True,text=True,check=True).stdout.strip()
    with tempfile.TemporaryDirectory(prefix='halo-luacodecs-',dir='/private/tmp') as temp:
        scratch=Path(temp);lua,seconds,versions=build_reference(args.redis_source,scratch)
        print(f'Reference build {seconds:.3f}s: {versions}',flush=True)
        binary=args.binary.resolve() if args.binary else scratch/'halo'
        build_seconds=0
        if not args.binary:
            start=time.monotonic();build=subprocess.run([args.compiler,'--graph',str(HERE.parent/'halo-e2e/modules.wfg'),'--entry','test','-o',str(binary)],cwd=ROOT,capture_output=True);build_seconds=time.monotonic()-start
            print(f'Halo build exit {build.returncode}, {build_seconds:.3f}s',flush=True)
            if build.returncode: print((build.stdout+build.stderr).decode());return 2
        wrapper=scratch/'reference.lua';wrapper.write_text(REFERENCE);chunk=scratch/'chunk.lua'
        for name,source in rows:
            chunk.write_text(source)
            ref=subprocess.run([str(lua),str(wrapper),str(chunk)],capture_output=True)
            if ref.returncode: raise RuntimeError(ref.stderr.decode())
            expected=json.loads(ref.stdout)
            actual_run=subprocess.run([str(binary),'seven','seven'],input=b'KEYS={};ARGV={}\0'+source.encode(),capture_output=True,timeout=30)
            actual=json.loads(actual_run.stdout) if actual_run.returncode==0 else {'exit':actual_run.returncode,'stderr':actual_run.stderr.decode('utf8','replace')}
            equal=actual_run.returncode==0 and e2e.canonical(actual)==e2e.canonical(expected)
            counts[(name.split('/')[0],equal)]+=1
            if not equal:
                failures.append((name,source,expected,actual));print('FAIL',name,repr(source),repr(expected)[:400],repr(actual)[:400],flush=True)
            if args.actual:
                args.actual.mkdir(parents=True,exist_ok=True);(args.actual/(name.replace('/','-')+'.json')).write_text(json.dumps({'source':source,'expected':expected,'actual':actual},ensure_ascii=True,indent=2))
        report=['# Redis Lua library compatibility results','',f'Local revision: `{revision}`. Host: `{__import__("platform").platform()}`.',f'Reference: Redis 7.0.15 bundled sources, all four libraries explicitly registered; `{versions}`.',f'Reference build {seconds:.3f}s. '+('Halo executable reused; no build performed during comparison.' if args.binary else f'Halo build {build_seconds:.3f}s.')+' Budget 7.',f'Compiler SHA-256: `{hashlib.sha256(Path(args.compiler).read_bytes()).hexdigest()}`.',f'Executable SHA-256: `{hashlib.sha256(binary.read_bytes()).hexdigest()}`.','', '| Library | Snippets | Matches | Mismatches |','| --- | ---: | ---: | ---: |']
        for group in sorted({n.split('/')[0] for n,_ in rows}):
            yes,no=counts[(group,True)],counts[(group,False)];report.append(f'| {group} | {yes+no} | {yes} | {no} |')
        report += ['',f'Total: {len(rows)} snippets; {len(rows)-len(failures)} matches; {len(failures)} mismatches.','', 'The comparator checks typed replies, binary bytes, and exact error text. Its fault sensitivity controls come from the existing end-to-end runner. The first return value is converted as Redis RESP2; snippets wrap multiple results where needed. No oracle fixtures were changed. Local libc is not Linux glibc: glibc-dependent behavior remains unqualified by this run.','', '## Mismatches','']
        for name,source,expected,actual in failures:
            report += [f'### {name}', '```lua',source,'```','Expected: `'+json.dumps(expected,ensure_ascii=True)+'`','Actual: `'+json.dumps(actual,ensure_ascii=True)+'`','']
        args.report.write_text('\n'.join(report))
        print(f'{len(rows)-len(failures)}/{len(rows)} matched',flush=True)
        return int(bool(failures))

if __name__=='__main__': raise SystemExit(main())
