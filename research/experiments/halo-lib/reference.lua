-- Reference-only Redis bootstrap. The scripts themselves are passed unchanged
-- to both interpreters. These 16-bit limbs translate src/rand.c's recurrence;
-- math.random's argument checks and scaling follow script_lua.c.
local x0,x1,x2=0x330e,0xabcd,0x1234
local function next31()
  local p=0xe66d*x0+11
  local q=math.floor(p/65536)+0xe66d*x1+0xdeec*x0
  local r=math.floor(q/65536)+0xe66d*x2+0xdeec*x1+5*x0
  x0=p%65536;x1=q%65536;x2=r%65536
  return x2*32768+math.floor(x1/2)
end
local function int(v,index,name)
  local x=tonumber(v)
  if not x then error("bad argument #"..index.." to '"..name.."' (number expected, got "..type(v)..")",3) end
  x=(x<0 and math.ceil(x) or math.floor(x))%4294967296
  if x>=2147483648 then x=x-4294967296 end
  return x
end
math.randomseed=function(seed)
  local n=int(seed,1,'randomseed')%4294967296
  x0=0x330e;x1=n%65536;x2=math.floor(n/65536)
end
math.random=function(...)
  local n=select('#',...);local a,b=...
  local r=(next31()%2147483647)/2147483647
  if n==0 then return r end
  if n>2 then error('wrong number of arguments',2) end
  local l,u=1,int(a,1,'random')
  if n==2 then l,u=u,int(b,2,'random');if l>u then error("bad argument #2 to 'random' (interval is empty)",2)end
  elseif u<1 then error("bad argument #1 to 'random' (interval is empty)",2)end
  return math.floor(r*(u-l+1))+l
end
local source=io.read('*a')
local f,e=loadstring(source,'@user_script')
if not f then io.stderr:write(e);os.exit(3) end
local ok,e=pcall(f)
if not ok then print('ERROR\t'..tostring(e)) end
