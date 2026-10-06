-- checks: Numeric and generic loops create captured iteration locals.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local a,b={},{}
for i=1,3 do a[i]=function() return i end end
for i,v in ipairs({10,20,30}) do b[i]=function() return v end end
return {a[1](),a[2](),a[3](),b[1](),b[2](),b[3]()}
