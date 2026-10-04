local lower = "lower"
local kept
do
  local middle = "kept"
  kept = function() return middle end
  local higher = "abandoned"
  local abandoned = function() return higher end
  abandoned = nil
  tostring(1)
  local capture_lower = function() return lower end
end
local reused = "wrong"
return kept()
