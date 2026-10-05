-- checks: Integer, bulk, missing bulk, array and status replies become Lua values.
-- KEYS: ["halo-oracle:api", "halo-oracle:api:missing", "halo-oracle:api:list"]
-- ARGV: []
-- expects: SET halo-oracle:api 6; RPUSH halo-oracle:api:list a b
-- setup: ["SET", "halo-oracle:api", "6"]
-- setup: ["RPUSH", "halo-oracle:api:list", "a", "b"]
local i=redis.call("INCR",KEYS[1])
local s=redis.call("GET",KEYS[1]); local n=redis.call("GET",KEYS[2])
local a=redis.call("LRANGE",KEYS[3],0,-1); local ok=redis.call("PING")
return {type(i),i,type(s),s,type(n),n,type(a),a,type(ok),ok.ok,ok}
