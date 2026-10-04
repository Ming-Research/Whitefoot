-- checks: __call receives the table as its first argument and can return many values.
-- KEYS: []
-- ARGV: []
-- expects: No pre-existing keys.
local t=setmetatable({base=7},{__call=function(self,x) return self.base+x,x*2 end})
return {t(5)}
