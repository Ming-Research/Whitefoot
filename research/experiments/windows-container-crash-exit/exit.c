#include <windows.h>
#include <stdlib.h>
#include <string.h>
static const char *mode;
static __attribute__((noinline)) unsigned long recurse(unsigned depth) {
  volatile char frame[57344];
  for (unsigned i = 0; i < sizeof frame; i += 4096) frame[i] = (char)depth;
  return recurse(depth + 1) + frame[0];
}
static LONG CALLBACK handler(EXCEPTION_POINTERS *e) {
  DWORD code = e->ExceptionRecord->ExceptionCode, written;
  if (code != EXCEPTION_STACK_OVERFLOW && code != 0xE0000001) return EXCEPTION_CONTINUE_SEARCH;
  WriteFile(GetStdHandle(STD_ERROR_HANDLE), "handled\n", 8, &written, NULL);
  if (strstr(mode, "exit")) ExitProcess(3);
  if (strstr(mode, "terminate")) TerminateProcess(GetCurrentProcess(), 3);
  if (strstr(mode, "fastfail")) __fastfail(7);
  abort();
}
static DWORD WINAPI body(void *arg) {
  ULONG guarantee = 64u * 1024u;
  (void)arg;
  SetThreadStackGuarantee(&guarantee);
  if (strstr(mode, "raise")) RaiseException(0xE0000001, 0, 0, NULL);
  if (strstr(mode, "plainfast")) __fastfail(7);
  return (DWORD)(recurse(0) & 1u);
}
int main(int argc, char **argv) {
  mode = argc > 1 ? argv[1] : "";
  if (strstr(mode, "abortonly")) abort();
  if (!strstr(mode, "nohandler")) AddVectoredExceptionHandler(1, handler);
  SIZE_T size = strstr(mode, "small") ? (SIZE_T)16 << 20 : (SIZE_T)1 << 30;
  HANDLE t = CreateThread(NULL, size, body, NULL, STACK_SIZE_PARAM_IS_A_RESERVATION, NULL);
  WaitForSingleObject(t, INFINITE);
  return 0;
}
