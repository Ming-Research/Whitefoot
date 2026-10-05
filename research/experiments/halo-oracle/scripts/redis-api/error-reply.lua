-- checks: redis.error_reply constructs a RESP error reply.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
-- error: true
return redis.error_reply("oracle helper error")
