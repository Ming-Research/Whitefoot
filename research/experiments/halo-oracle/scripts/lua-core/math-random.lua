-- checks: Redis deterministic default seed and explicit reseeding.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
-- note: Redis 7.0.15 uses redisLrand48 with default seed 0x1234abcd; it does not reseed each EVAL. Explicitly reset that seed for isolated comparison.
math.randomseed(305441741)
local a,b=math.random(),math.random(1,1000000)
math.randomseed(12345)
local c,d,e=math.random(),math.random(10),math.random(-5,5)
math.randomseed(12345)
return {tostring(a),b,tostring(c),d,e,tostring(math.random()),math.random(10),math.random(-5,5)}
