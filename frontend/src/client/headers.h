#ifndef CLIENT_HEADERS
#define CLIENT_HEADERS

#if defined(_WIN32)
#else
   #ifndef USE_WIN_DWORD_RANGE
      #define USE_WIN_DWORD_RANGE
   #endif
#endif

typedef void VOID;
typedef VOID *PVOID;
typedef VOID *LPVOID;

#ifndef USE_WIN_DWORD_RANGE
#ifdef __APPLE__
#include <stdint.h>
typedef uint32_t      DWORD;
#else
typedef unsigned long DWORD;
#endif
#else
typedef unsigned int DWORD;
#endif

typedef DWORD *LPDWORD;

typedef unsigned int UINT32;

typedef unsigned int UINT;

typedef int BOOL;

const BOOL TRUE = 1;
const BOOL FALSE = 0;

#endif
