-- checks: struct packs explicit-endian signed and unsigned integer widths.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local p=struct.pack(">i2I2",-123,65535)
local a,b,nextpos=struct.unpack(">i2I2",p)
local hex=string.gsub(p,".",function(c) return string.format("%02x",string.byte(c)) end)
return {hex,a,b,nextpos}
