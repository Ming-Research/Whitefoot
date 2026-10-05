-- checks: redis.sha1hex hashes empty, ASCII and embedded NUL byte strings.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {redis.sha1hex(""),redis.sha1hex("abc"),redis.sha1hex("a\000b")}
