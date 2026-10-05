-- Redis 7.0.15's standalone Lua 5.1; leave its default GC policy enabled.
local n = tonumber(arg[1])
assert(#arg == 1 and n and n == math.floor(n) and n >= 4 and n <= 16)
local function tree(depth)
    if depth == 0 then return {} end
    return {tree(depth - 1), tree(depth - 1)}
end
local function check(node)
    if node[1] then return 1 + check(node[1]) + check(node[2]) end
    return 1
end
local stretch = tree(n + 1)
io.write(string.format("stretch tree of depth %d\t check: %d\n", n + 1, check(stretch)))
stretch = nil
local long_lived = tree(n)
for d = 4, n, 2 do
    local iterations, sum = 2 ^ (n - d + 4), 0
    for i = 1, iterations do
        local temporary = tree(d)
        sum = sum + check(temporary)
    end
    io.write(string.format("%d\t trees of depth %d\t check: %d\n", iterations, d, sum))
end
io.write(string.format("long lived tree of depth %d\t check: %d\n", n, check(long_lived)))
