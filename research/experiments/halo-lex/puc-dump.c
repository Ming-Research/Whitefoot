/* PUC's lexer is included unchanged except for recording token-start metadata.
 * run.py writes oracle-llex.c in scratch, from the pinned llex.c. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "lua.h"
#include "lauxlib.h"
#include "ldo.h"
#include "lstate.h"
#include "lparser.h"
#include "ltable.h"
#include "lstring.h"
#include "lzio.h"
#include "llex.h"
static const char *input, *source_name;
static size_t input_length, token_start;
static int token_line, supplied, source_line = 1;
static size_t offset(LexState *ls) {
  return ls->current == EOZ ? input_length : (size_t)(ls->z->p - input - 1);
}
#include "oracle-llex.c"
static const char *reader(lua_State *L, void *data, size_t *size) {
  (void)L; (void)data;
  if (supplied) { *size = 0; return NULL; }
  supplied = 1; *size = input_length; return input;
}
static void hex(const char *bytes, size_t n) {
  if (!n) { putchar('-'); return; }
  for (size_t i = 0; i < n; i++) printf("%02x", (unsigned char)bytes[i]);
}
static void dump(lua_State *L, void *ud) {
  Mbuffer *buffer = ud;
  LexState ls = {0}; FuncState fs = {0}; ZIO z;
  ls.buff = buffer;
  luaZ_init(L, &z, reader, NULL);
  lua_pushstring(L, source_name); /* Keep the diagnostic source rooted across GC. */
  luaX_setinput(L, &ls, &z, luaS_new(L, source_name));
  ls.linenumber = source_line;
  fs.h = luaH_new(L, 0, 0);
  sethvalue(L, L->top, fs.h); L->top++;
  ls.fs = &fs;
  for (;;) {
    SemInfo info;
    int kind = llex(&ls, &info);
    ls.t.token = kind;
    size_t end = offset(&ls);
    printf("T\t%d\t%zu\t%zu\t%d\t", kind, token_start, end, token_line);
    if (kind == TK_STRING) hex(getstr(info.ts), info.ts->tsv.len);
    else if (kind == TK_NAME || kind == TK_NUMBER) hex(input + token_start, end - token_start);
    else putchar('-');
    putchar('\n');
    if (kind == TK_EOS) break;
  }
}
int main(int argc, char **argv) {
  if (argc != 2 && argc != 3 && argc != 4) return 2;
  source_name = argc >= 3 ? argv[2] : "=fixture";
  if (argc == 4) source_line = atoi(argv[3]);
  FILE *f = fopen(argv[1], "rb"); if (!f) return 2;
  if (fseek(f, 0, SEEK_END)) return 2;
  long length = ftell(f); if (length < 0 || fseek(f, 0, SEEK_SET)) return 2;
  input_length = (size_t)length;
  char *owned = malloc(input_length + 1); if (!owned) return 2;
  if (fread(owned, 1, input_length, f) != input_length) return 2;
  fclose(f); input = owned;
  lua_State *L = luaL_newstate(); if (!L) return 2;
  Mbuffer buffer; luaZ_initbuffer(L, &buffer);
  int status = luaD_rawrunprotected(L, dump, &buffer);
  if (status) {
    const char *message = lua_tostring(L, -1);
    if (!message || status != LUA_ERRSYNTAX) return 3;
    printf("E\t"); hex(message, strlen(message)); putchar('\n');
  }
  luaZ_freebuffer(L, &buffer);
  lua_close(L); free(owned); return 0;
}
