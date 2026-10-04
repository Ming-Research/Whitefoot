redis.call("SET", "gc-survivor", "alive")
local payload = string.rep("x", 1048576)
local growing = {}
while true do
  growing[#growing + 1] = payload .. tostring(#growing + 1)
end
