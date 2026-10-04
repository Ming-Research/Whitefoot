-- The requested depth bound is additional to the bundled Lua 5.1 matcher.
local results={}
local function probe(name, source, pattern)
  local ok,value=pcall(string.match,source,pattern)
  if ok then results[#results+1]=name.."|true|"..#value
  else
    local i=string.find(value,": ",1,true)
    if i then value=string.sub(value,i+2) end
    results[#results+1]=name.."|false|"..value
  end
end
probe("optional-199",string.rep("a",199),string.rep("a?",199))
probe("optional-200",string.rep("a",200),string.rep("a?",200))
probe("greedy-199","",string.rep("a*",199))
probe("greedy-200","",string.rep("a*",200))
probe("minimal-199","",string.rep("a-",199))
probe("minimal-200","",string.rep("a-",200))
probe("tail-1000",string.rep("a",1000),string.rep("a",1000))
return results
