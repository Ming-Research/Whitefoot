-- checks: Patterns support captures, balanced matching and frontier boundaries.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
return {string.match("abc123","(%a+)(%d+)"),{string.match("abc123","(%a+)(%d+)")},string.match("x(a(b)c)y","%b()"),string.match("a cat!","%f[%a]cat%f[%A]"),string.match("abc","%d+") == nil}
