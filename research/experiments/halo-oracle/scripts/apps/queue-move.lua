-- checks: LPOP/RPUSH moves jobs in order and stops on an empty source.
-- KEYS: ["halo-oracle:app:source", "halo-oracle:app:dest"]
-- ARGV: []
-- expects: RPUSH halo-oracle:app:source job-a job-b; RPUSH halo-oracle:app:dest older
-- setup: ["RPUSH", "halo-oracle:app:source", "job-a", "job-b"]
-- setup: ["RPUSH", "halo-oracle:app:dest", "older"]
local function move() local job=redis.call("LPOP",KEYS[1]); if job then redis.call("RPUSH",KEYS[2],job) end; return job end
return {move(),move(),move(),redis.call("LRANGE",KEYS[1],0,-1),redis.call("LRANGE",KEYS[2],0,-1)}
