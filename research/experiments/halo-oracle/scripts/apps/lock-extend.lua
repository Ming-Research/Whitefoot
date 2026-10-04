-- checks: PEXPIRE extends a lock only for its current owner.
-- KEYS: ["halo-oracle:app:lock"]
-- ARGV: ["owner", "60000"]
-- expects: SET halo-oracle:app:lock owner
-- setup: ["SET", "halo-oracle:app:lock", "owner"]
local function extend(owner) if redis.call("GET",KEYS[1])==owner then return redis.call("PEXPIRE",KEYS[1],ARGV[2]) end; return 0 end
return {extend("other"),extend(ARGV[1]),redis.call("GET",KEYS[1])}
