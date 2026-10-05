local N = 1000000
local t = { alpha = 17, beta = 23, gamma = 31, delta = 43 }
local keys = { "alpha", "beta", "gamma", "delta" }
local sum = 0
for i = 1, N do sum = sum + t[keys[(i - 1) % 4 + 1]] end
print(sum)
