-- checks: Sorted-set limiter removes old scores and counts a fixed-time window.
-- KEYS: ["halo-oracle:app:window"]
-- ARGV: ["1000", "100", "new", "2"]
-- expects: ZADD halo-oracle:app:window 850 old 950 recent
-- setup: ["ZADD", "halo-oracle:app:window", "850", "old", "950", "recent"]
-- needs: ZREMRANGEBYSCORE
local now=tonumber(ARGV[1]); local window=tonumber(ARGV[2])
local removed=redis.call("ZREMRANGEBYSCORE",KEYS[1],"-inf",now-window)
redis.call("ZADD",KEYS[1],now,ARGV[3])
local n=redis.call("ZCARD",KEYS[1])
return {removed,n,n<=tonumber(ARGV[4]),redis.call("ZSCORE",KEYS[1],ARGV[3])}
