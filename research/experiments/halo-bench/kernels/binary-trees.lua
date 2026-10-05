local N = 16
local function tree(item, depth)
  if depth == 0 then return { item = item } end
  return { item = item, left = tree(2 * item - 1, depth - 1), right = tree(2 * item, depth - 1) }
end
local function check(t)
  if not t.left then return t.item end
  return t.item + check(t.left) - check(t.right)
end
local sum = check(tree(0, N + 1))
local long = tree(0, N)
for depth = 4, N, 2 do
  local iterations = 2 ^ (N - depth + 4)
  for i = 1, iterations do
    sum = sum + check(tree(i, depth)) + check(tree(-i, depth))
  end
end
print(sum + check(long))
