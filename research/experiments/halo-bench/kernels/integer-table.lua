local N = 10000000
local t = {}
for i = 1, N do t[i] = i end
local sum = 0
for i = 1, N do sum = sum + t[i] end
print(sum)
