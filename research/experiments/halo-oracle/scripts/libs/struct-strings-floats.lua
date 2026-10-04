-- checks: struct packs fixed strings and doubles with explicit endian and offsets.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local p=struct.pack("<c3d","a\000b",1.25)
local s,n,nextpos=struct.unpack("<c3d",p)
local hex=string.gsub(p,".",function(c) return string.format("%02x",string.byte(c)) end)
return {hex,s,tostring(n),nextpos}
