-- The reference executable has no dynamic loader. Its fallback transports
-- inputs and extracts finite results with exact ldexp/frexp arithmetic.
-- The companion host installs bits through the same reference liblua.a so NaN
-- payloads can also be read; these outputs are cross-checked by compare.py.
local bits = _G.bits
if not bits then
    bits = {}
    function bits.frombits(text)
        local high, low = tonumber(text:sub(1, 8), 16), tonumber(text:sub(9, 16), 16)
        local exponent = math.floor(high / 1048576) % 2048
        local mantissa = (high % 1048576) * 4294967296 + low
        local x
        if exponent == 2047 then
            x = mantissa == 0 and math.huge or 0 / 0
        elseif exponent == 0 then
            x = math.ldexp(mantissa, -1074)
        else
            x = math.ldexp(mantissa + 4503599627370496, exponent - 1075)
        end
        return high >= 2147483648 and -x or x
    end
    function bits.tobits(x)
        local sign = 0
        if x < 0 or (x == 0 and 1 / x < 0) then sign = 2147483648 end
        if x ~= x then
            if tostring(x):sub(1, 1) == "-" then sign = 2147483648 end
            return string.format("%08x00000000", sign + 2146959360)
        end
        x = math.abs(x)
        if x == math.huge then
            return string.format("%08x00000000", sign + 2146435072)
        end
        local fraction, exponent = math.frexp(x)
        local mantissa, biased
        if exponent <= -1022 then
            mantissa, biased = math.ldexp(x, 1074), 0
        elseif x == 0 then
            mantissa, biased = 0, 0
        else
            mantissa, biased = math.ldexp(fraction, 53) - 4503599627370496, exponent + 1022
        end
        local high = sign + biased * 1048576 + math.floor(mantissa / 4294967296)
        local low = mantissa % 4294967296
        return string.format("%08x%08x", high, low)
    end
end
local power = bits.pow or function(x, y) return x ^ y end
local function unhex(text)
    return (text:gsub("..", function(byte)
        return string.char(tonumber(byte, 16))
    end))
end
for line in io.lines() do
    local operation, rest = line:sub(1, 1), line:sub(3)
    local result
    if operation == "S" then
        local value = tonumber(unhex(rest))
        result = value and bits.tobits(value) or "nil"
    else
        local first, second = rest:match("^(%x+) ?(%x*)$")
        local x = bits.frombits(first)
        if operation == "F" then
            result = tostring(x)
        elseif operation == "P" then
            result = bits.tobits(power(x, bits.frombits(second)))
        elseif operation == "M" then
            result = bits.tobits(math.fmod(x, bits.frombits(second)))
        elseif operation == "L" then
            result = bits.tobits(math.floor(x))
        elseif operation == "C" then
            result = bits.tobits(math.ceil(x))
        else
            error("unknown operation")
        end
    end
    io.write(result, "\n")
end
