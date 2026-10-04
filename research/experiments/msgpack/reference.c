/* Scratch-built oracle: compile with the unmodified Redis Lua sources and
 * bundled liblua.a. The Python runner supplies their locations. */
#include <stdio.h>
#include "lua_cmsgpack.c"

static uint64_t word(const unsigned char *p) {
    uint64_t n = 0;
    for (unsigned i = 0; i < 8; ++i) n |= (uint64_t)p[i] << (i * 8);
    return n;
}

int main(void) {
    lua_State *L = luaL_newstate();
    unsigned char header[17];
    while (fread(header, 1, sizeof(header), stdin) == sizeof(header)) {
        unsigned op = header[0];
        uint64_t bits = word(header + 1), size = word(header + 9);
        if (!op) { lua_close(L); return 0; }
        if (size > 1048576) return 2;
        unsigned char *data = malloc(size ? (size_t)size : 1);
        if (!data || fread(data, 1, (size_t)size, stdin) != size) return 3;
        mp_buf *buf = mp_buf_new(L);
        switch (op) {
        case 1: mp_encode_lua_null(L, buf); break;
        case 2:
            lua_pushboolean(L, bits != 0);
            mp_encode_lua_type(L, buf, 0);
            break;
        case 3:
            if (bits > INT64_MAX) return 4;
            mp_encode_int(L, buf, (int64_t)bits);
            break;
        case 4: {
            int64_t n;
            memcpy(&n, &bits, 8);
            mp_encode_int(L, buf, n);
            break;
        }
        case 5: {
            double n;
            memcpy(&n, &bits, 8);
            lua_pushnumber(L, n);
            mp_encode_lua_type(L, buf, 0);
            break;
        }
        case 8:
            lua_pushlstring(L, (const char *)data, (size_t)size);
            mp_encode_lua_type(L, buf, 0);
            break;
        case 11: mp_encode_array(L, buf, bits); break;
        case 12: mp_encode_map(L, buf, bits); break;
        default: return 5;
        }
        uint64_t count = buf->len;
        unsigned char prefix[8];
        for (unsigned i = 0; i < 8; ++i) prefix[i] = (count >> (8 * i)) & 255;
        if (fwrite(prefix, 1, 8, stdout) != 8 ||
            fwrite(buf->b, 1, buf->len, stdout) != buf->len) return 6;
        mp_buf_free(L, buf);
        free(data);
    }
    return 7;
}
