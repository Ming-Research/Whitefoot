local N = 1000000
local t = {}
local seed = 42
for i = 1, N do
  seed = (seed * 48271) % 2147483647
  t[i] = seed
end
table.sort(t)
local sum = 0
for i = 1, N do
  if i > 1 then assert(t[i - 1] <= t[i]) end
  sum = sum + t[i]
end
print(sum)
