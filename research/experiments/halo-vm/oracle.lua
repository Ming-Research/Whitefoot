local function fib(n) if n < 2 then return n end return fib(n-1)+fib(n-2) end
print('fib',fib(20))
local sum=0 for i=1,100 do sum=sum+i end print('numeric_for',sum)
local n=0 local function a() n=n+1 return n end local function b() n=n+1 return n end
print('counter',a(),b())
local t={} t[1]=42 t.key=73 print('tables',t[1]+t.key)
local fallback={answer=9}
local chain=setmetatable({},{__index=fallback})
local mt={__index=chain,__add=function(x,y) type(x); type(x); return x.n+y.n end}
local x=setmetatable({n=3},mt) local y=setmetatable({n=5},mt)
print('meta',x.answer+(x+y))
local ok,e=pcall(function() error({code=1}) end)
local ok2,e2=pcall(function() local function inner() error({code=2}) end inner() end)
print('protected_error',ok,e.code+e2.code,ok2)
local function vararg(...) return select('#',...) end
print('vararg',vararg(1,2,3,4))
local function loop(n,acc) if n<1 then return acc end return loop(n-1,acc+1) end
print('tail',loop(100000,0))

local errors=assert(loadstring("return function(level)\n local function child() error('oops',level) end\n return pcall(child)\nend",'@user_script'))()
for level=0,3 do print('error_level_'..level,errors(level)) end
local metamethod_error=assert(loadstring("return function() local mt\n local function child() local t=setmetatable({},mt); return t+t end\n mt={__add=function()\n error('oops',2) end}\n return pcall(child)\nend",'@user_script'))()
print('error_callback',metamethod_error())
local concat_error=assert(loadstring("return function() local mt\n local function child() local t=setmetatable({},mt); return t..t end\n mt={__concat=function()\n error('oops',2) end}\n return pcall(child)\nend",'@user_script'))()
print('error_concat',concat_error())
