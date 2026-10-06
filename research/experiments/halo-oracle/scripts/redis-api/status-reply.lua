-- checks: redis.status_reply constructs a RESP status reply.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return redis.status_reply("oracle helper status")
