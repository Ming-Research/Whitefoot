# Firn

Firn, the Redis-compatible server written in Whitefoot: what its deployment and scripting milestones demonstrate.

### decision firn-serves-standalone-redis-workloads
Firn's first deployment milestone is standalone Redis workloads, cache and session storage and scripted conditional updates, for unchanged applications through their ordinary clients.
- rejected: an agent proposed releasing a benchmark-only subset of Redis; the owner rejected it, because benchmark commands alone do not show practical use or tell a language advantage from omitted work.

### decision firn-runs-lua-in-whitefoot
Firn runs Redis's Lua scripts on Halo, a Lua interpreter written in Whitefoot, built first as a vertical slice tested against Redis's interpreter.
- rejected: an agent proposed embedding a native Lua engine as the language evidence; the owner rejected it, because dynamic values, closures, object reclamation and atomic command composition exercise language capabilities that a native engine would not show.