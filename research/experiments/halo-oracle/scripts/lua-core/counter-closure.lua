-- checks: Separate counter closures retain independent upvalue state.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local function make() local n=0; return function() n=n+1; return n end end
local a,b=make(),make()
return {a(),a(),b(),a(),b()}
