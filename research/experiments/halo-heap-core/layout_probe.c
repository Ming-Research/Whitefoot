/* Optional attribution probe: linked to the same PUC Lua library as the oracle.
   Inspects table sizes without changing table operations or allocation. */
#include <stdio.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"
#include "lstate.h"
#include "lobject.h"
#include "ltable.h"

static int observe(lua_State *L) {
    Table *t = hvalue(L->base);
    int step = (int)lua_tonumber(L, 2);
    if (step >= 185 && step <= 225) {
        int occupied = 0, live = 0, i;
        for (i = 0; i < sizenode(t); i++) {
            occupied += !ttisnil(gkey(gnode(t, i)));
            live += !ttisnil(gval(gnode(t, i)));
        }
        fprintf(stderr, "%d %d %d %d %d\n", step, t->sizearray,
                sizenode(t), occupied, live);
    }
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    lua_State *L = luaL_newstate();
    if (!L) return 3;
    luaL_openlibs(L);
    lua_pushcfunction(L, observe);
    lua_setglobal(L, "observe");
    int status = luaL_loadfile(L, argv[1]);
    if (!status) status = lua_pcall(L, 0, 0, 0);
    if (status) fprintf(stderr, "%s\n", lua_tostring(L, -1));
    lua_close(L);
    return status != 0;
}
