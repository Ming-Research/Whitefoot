-- checks: A Redis error caught by Lua pcall is a string rather than a reply table.
-- KEYS: ["halo-oracle:api"]
-- ARGV: []
-- expects: SET halo-oracle:api not-an-integer
-- setup: ["SET", "halo-oracle:api", "not-an-integer"]
local ok,e=pcall(function() return redis.call("INCR",KEYS[1]) end)
return {ok,type(e),e}
