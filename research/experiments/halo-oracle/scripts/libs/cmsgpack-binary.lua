-- checks: MessagePack exact scalar bytes and binary strings survive packing.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local s="a\000b"
local packed=cmsgpack.pack({1,-1,true,false,s})
local hex=string.gsub(packed,".",function(c) return string.format("%02x",string.byte(c)) end)
local t=cmsgpack.unpack(packed)
return {hex,t,#t[5]}
