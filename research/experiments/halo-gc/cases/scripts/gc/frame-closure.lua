return (function(x)
  return function()
    local scratch = {}
    for i = 1, 4 do scratch[i] = i end
    return x
  end
end)(string.rep("q", 3))()
