-- Load identical corpus bytes with the same source name as Halo.
local source=io.read('*a')
local f,e=loadstring(source,'@user_script')
if not f then io.stderr:write(e);os.exit(3) end
local ok,e=pcall(f)
if not ok then print('ERROR\t'..tostring(e)) end
