-- checks: Lock release deletes only a matching owner and handles a missing lock.
-- KEYS: ["halo-oracle:app:lock"]
-- ARGV: ["owner"]
-- expects: SET halo-oracle:app:lock owner
-- setup: ["SET", "halo-oracle:app:lock", "owner"]
local function release(owner) if redis.call("GET",KEYS[1])==owner then return redis.call("DEL",KEYS[1]) end; return 0 end
local wrong=release("other"); local right=release(ARGV[1]); local missing=release(ARGV[1])
return {wrong,right,missing,redis.call("GET",KEYS[1])}
