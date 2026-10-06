-- checks: redis.call returns successful write and read replies.
-- KEYS: ["halo-oracle:api"]
-- ARGV: ["41"]
-- expects: No pre-existing keys.
local s=redis.call("SET",KEYS[1],ARGV[1])
return {s,redis.call("GET",KEYS[1]),redis.call("INCR",KEYS[1])}
