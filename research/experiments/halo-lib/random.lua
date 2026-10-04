math.randomseed(0); print('random',math.random(),math.random(10),math.random(-3,7))
math.randomseed(123456);for i=1,12 do print('rng',i,math.random(),math.random(-1000,1000)) end
math.randomseed(-1); print('negseed',math.random(),math.random(1))
print('badrange',pcall(function() return math.random(0) end))
print('badcount',pcall(function() return math.random(1,2,3) end))
