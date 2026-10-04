/* The oracle bridge only transports binary64 bits through the Lua C API.
 * Lua itself performs tostring, tonumber, ^, fmod, floor and ceil. */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

_Static_assert(sizeof(lua_Number) == sizeof(uint64_t), "binary64 Lua required");

static int frombits(lua_State *state) {
    const char *text = luaL_checkstring(state, 1);
    unsigned long long raw;
    if (strlen(text) != 16 || sscanf(text, "%llx", &raw) != 1)
        return luaL_error(state, "expected sixteen hexadecimal digits");
    uint64_t bits = (uint64_t)raw;
    lua_Number number;
    memcpy(&number, &bits, sizeof(number));
    lua_pushnumber(state, number);
    return 1;
}

static int tobits(lua_State *state) {
    lua_Number number = luaL_checknumber(state, 1);
    uint64_t bits;
    char text[17];
    memcpy(&bits, &number, sizeof(bits));
    snprintf(text, sizeof(text), "%016llx", (unsigned long long)bits);
    lua_pushlstring(state, text, 16);
    return 1;
}

#ifdef HALO_MUSL_REF
extern double halo_musl_pow(double, double);
static int power(lua_State *state) {
    lua_Number x = luaL_checknumber(state, 1);
    lua_Number y = luaL_checknumber(state, 2);
    lua_pushnumber(state, halo_musl_pow(x, y));
    return 1;
}
#endif

int luaopen_bits(lua_State *state) {
    const luaL_Reg functions[] = {
        {"frombits", frombits}, {"tobits", tobits},
#ifdef HALO_MUSL_REF
        {"pow", power},
#endif
        {NULL, NULL}
    };
    luaL_register(state, "bits", functions);
    return 1;
}

/* Link the unchanged reference liblua.a: this only installs the bit bridge. */
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    lua_State *state = luaL_newstate();
    if (!state) return 2;
    luaL_openlibs(state);
    luaopen_bits(state);
    lua_pop(state, 1);
    int status = luaL_dofile(state, argv[1]);
    if (status) fprintf(stderr, "%s\n", lua_tostring(state, -1));
    lua_close(state);
    return status ? 1 : 0;
}
