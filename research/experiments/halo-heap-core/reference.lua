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

local keys = {}
for i = 0, 95 do keys[i] = i end
keys[96] = -1.5
keys[97] = -2.5
keys[98] = 1.5
keys[99] = 2.5
keys[100] = math.huge
keys[101] = -math.huge
keys[102] = 2^-1074
keys[103] = -2^-1074
keys[104] = (2-2^-52)*2^1023
keys[105] = -(2-2^-52)*2^1023
keys[106] = 1+(1-2^-32)*2^-20
keys[107] = -(1+(1-2^-32)*2^-20)
keys[108] = 2^31
keys[109] = -2^31
keys[110] = 2^53
keys[111] = -2^53
for i = 0, 31 do keys[112+i] = string.char(i)..string.rep('a',95) end
for i = 0, 15 do keys[144+i] = string.char(0,i,255) end
keys[160], keys[161] = false, true
local ids = {}
for i = 0, 161 do ids[keys[i]] = i end
for case = 0, 7 do
  local t, seed = {}, 12345 + case * 997
  for step = 0, 1023 do
    seed = (seed * 1664525 + 1013904223) % 4294967296
    local id, op = math.floor(seed / 256) % 162, seed % 8
    t[keys[id]] = op < 6 and step + 1 or nil
    if step % 128 == 127 then
      local cursor, count = nil, 0
      while true do
        local k, v = next(t, cursor)
        if k == nil then break end
        print(3, case, step, count, ids[k], v, #t)
        count, cursor = count + 1, k
      end
      print(4, case, step, count, #t, 0, 0)
    end
  end
end
