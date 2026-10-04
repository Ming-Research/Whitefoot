local seed, t = 12345, {}
for step = 0, 2047 do
  seed = (seed * 1664525 + 1013904223) % 4294967296
  local key, op = math.floor(seed / 256) % 96, seed % 8
  local val = op < 6 and step + 1 or nil
  t[key] = val
  local got = t[key] or 0
  local count, sum, cursor = 0, 0, nil
  while true do
    local k, v = next(t, cursor)
    if k == nil then break end
    count, sum, cursor = count + 1, sum + k * v, k
    if step % 31 == 0 and k % 3 == 0 then t[k] = nil end
  end
  if observe then observe(t, step) end
  print(0, step, key, got, #t, count, sum)
end
for case = 0, 5 do
  local t = {}
  for j = 0, 31 do
    local key = j + 1
    if case == 1 then key = 32 - j end
    if case == 2 then key = j % 2 == 0 and math.floor(j / 2) + 1 or 32 - math.floor(j / 2) end
    if case == 3 then key = key * 2 end
    if case == 4 then key = (j * 7) % 32 + 1 end
    if case == 5 then key = (j * 13) % 32 + 1 end
    t[key] = 1
    print(1, case, j, key, #t, 0, 0)
  end
  for j = 0, 15 do
    local key = j * 2 + 1
    t[key] = nil
    print(2, case, j, key, #t, 0, 0)
  end
end
