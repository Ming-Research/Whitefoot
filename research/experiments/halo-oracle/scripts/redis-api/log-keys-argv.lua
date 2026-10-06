-- checks: redis.log is allowed; KEYS and ARGV remain ordered strings.
-- KEYS: ["halo-oracle:api", "halo-oracle:api:second"]
-- ARGV: ["17", "hello world", ""]
-- expects: No pre-existing keys.
redis.log(redis.LOG_NOTICE,"halo oracle log")
return {#KEYS,#ARGV,KEYS,ARGV,type(ARGV[1])}
