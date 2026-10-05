-- checks: find plain mode, pattern captures, start offsets and no match.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local a,b=string.find("a.b.c",".",1,true)
local c,d,e=string.find("id=123;","id=(%d+)")
return {a,b,c,d,e,string.find("abcabc","abc",2),string.find("abc","z") == nil}
