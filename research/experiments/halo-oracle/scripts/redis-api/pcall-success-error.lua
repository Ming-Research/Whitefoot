-- checks: redis.pcall returns successes and error tables without raising.
-- KEYS: ["halo-oracle:api"]
-- ARGV: []
-- expects: No pre-existing keys.
local a=redis.pcall("SET",KEYS[1],"x")
local b=redis.pcall("INCR",KEYS[1])
return {a,b,type(b),b.err}
