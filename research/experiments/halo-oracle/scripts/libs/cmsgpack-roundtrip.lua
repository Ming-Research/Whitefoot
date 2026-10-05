-- checks: MessagePack round-trips maps, arrays, booleans, integers and strings.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local x={name="halo",items={1,-2,"x",true,false}}
local packed=cmsgpack.pack(x); local y=cmsgpack.unpack(packed)
return {type(packed),y.name,y.items,y.name==x.name}
