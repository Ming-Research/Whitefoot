local N = 1000000
local parts = { "ab", "cd", "ef", "gh" }
local sum = 0
for i = 1, N do
  local s = parts[(i - 1) % 4 + 1] .. "-" .. parts[i % 4 + 1]
  sum = sum + #s
end
print(sum)
