-- checks: INCR and first-hit PEXPIRE enforce a limit without clock-valued output.
-- KEYS: ["halo-oracle:app:rate"]
-- ARGV: ["2", "60000"]
-- expects: No pre-existing keys.
local function hit()
 local n=redis.call("INCR",KEYS[1])
 local expiry=0; if n==1 then expiry=redis.call("PEXPIRE",KEYS[1],ARGV[2]) end
 return {n,n<=tonumber(ARGV[1]),expiry}
end
return {hit(),hit(),hit(),redis.call("GET",KEYS[1])}
