-- checks: redis.call raises command errors to the EVAL reply.
-- KEYS: ["halo-oracle:api"]
-- ARGV: []
-- expects: No pre-existing keys.
-- error: true
return redis.call("GET",KEYS[1],"extra")
