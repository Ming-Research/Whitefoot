-- checks: gsub table and function replacements preserve false or nil matches.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local a,n=string.gsub("a b c","%a",{a="A",b=false})
local b,m=string.gsub("1 2 3","%d",function(s) if s=="2" then return nil end; return tonumber(s)*10 end)
return {a,n,b,m}
