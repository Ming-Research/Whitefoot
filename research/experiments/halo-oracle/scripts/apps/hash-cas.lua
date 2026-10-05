-- checks: Hash compare-and-set leaves mismatches untouched and reports a match.
-- KEYS: ["halo-oracle:app:hash"]
-- ARGV: ["state", "pending", "done"]
-- expects: HSET halo-oracle:app:hash state pending
-- setup: ["HSET", "halo-oracle:app:hash", "state", "pending"]
local function cas(old,new) if redis.call("HGET",KEYS[1],ARGV[1])~=old then return 0 end; redis.call("HSET",KEYS[1],ARGV[1],new); return 1 end
return {cas("wrong","bad"),cas(ARGV[2],ARGV[3]),redis.call("HGET",KEYS[1],ARGV[1]),redis.call("HGET",KEYS[1],"missing")}
