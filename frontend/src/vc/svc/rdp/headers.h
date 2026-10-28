#if defined(_WIN32)
#else
   #ifndef USE_WIN_DWORD_RANGE
      #define USE_WIN_DWORD_RANGE
   #endif
#endif

typedef void VOID;
typedef VOID *PVOID;
typedef VOID *LPVOID;

typedef unsigned long ULONG;
typedef ULONG *PULONG;

typedef unsigned int UINT;
typedef unsigned int UINT32;

typedef int INT;

typedef char CHAR;
typedef CHAR *PCHAR;

typedef int BOOL;

const BOOL TRUE = 1;
const BOOL FALSE = 0;

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


/*
 * Reference: http://msdn.microsoft.com/en-us/library/windows/desktop/aa383564(v=vs.85).aspx
 */

#define CHANNEL_NAME_LEN 7

typedef struct _CHANNEL_DEF {
        char  name[CHANNEL_NAME_LEN+1];
        ULONG options;
} CHANNEL_DEF, *PCHANNEL_DEF, **PPCHANNEL_DEF;

typedef VOID (*VirtualChannelInitEvent) (LPVOID pInitHandle,
                                         UINT event,
                                         LPVOID pData,
                                         UINT dataLength
                                        );

typedef VirtualChannelInitEvent PCHANNEL_INIT_EVENT_FN;

typedef UINT (*VirtualChannelInit) (LPVOID *ppInitHandle,
                                    PCHANNEL_DEF pChannel,
                                    INT channelCount,
                                    ULONG versionRequested,
                                    PCHANNEL_INIT_EVENT_FN pChannelInitEventProc
                                   );

typedef VirtualChannelInit PVIRTUALCHANNELINIT;

typedef VOID (*VirtualChannelOpenEvent) (DWORD openHandle,
                                         UINT event,
                                         LPVOID pData,
                                         UINT32 dataLength,
                                         UINT32 totalLength,
                                         UINT32 dataFlags
                                        );

typedef VirtualChannelOpenEvent PCHANNEL_OPEN_EVENT_FN;

typedef UINT (*VirtualChannelOpen) (LPVOID pInitHandle,
                                    LPDWORD pOpenHandle,
                                    PCHAR pChannelName,
                                    PCHANNEL_OPEN_EVENT_FN pChannelOpenEventProc
                                   );

typedef VirtualChannelOpen PVIRTUALCHANNELOPEN;

typedef UINT (*VirtualChannelClose) (DWORD openHandle);

typedef VirtualChannelClose PVIRTUALCHANNELCLOSE;

typedef UINT (*VirtualChannelWrite) (DWORD openHandle,
                                     LPVOID pData,
                                     ULONG dataLength,
                                     LPVOID pUserData
                                    );

typedef VirtualChannelWrite PVIRTUALCHANNELWRITE;

typedef struct  _CHANNEL_ENTRY_POINTS {
        DWORD                cbSize;
        DWORD                protocolVersion;
        PVIRTUALCHANNELINIT  pVirtualChannelInit;
        PVIRTUALCHANNELOPEN  pVirtualChannelOpen;
        PVIRTUALCHANNELCLOSE pVirtualChannelClose;
        PVIRTUALCHANNELWRITE pVirtualChannelWrite;
} CHANNEL_ENTRY_POINTS, *PCHANNEL_ENTRY_POINTS;

typedef BOOL (*VirtualChannelEntryMSDN) (PCHANNEL_ENTRY_POINTS pEntryPoints);

typedef VirtualChannelEntryMSDN PVIRTUALCHANNELENTRY;


typedef VOID (*VirtualChannelInitEventEx) (LPVOID lpUserParam,
                                           LPVOID pInitHandle,
                                           UINT event,
                                           LPVOID pData,
                                           UINT dataLength
                                           );

typedef VirtualChannelInitEventEx PCHANNEL_INIT_EVENT_EX_FN;

typedef UINT (*VirtualChannelInitEx_Freerdp) (LPVOID lpUserParam,
                                      LPVOID clientContext,
                                      LPVOID pInitHandle,
                                      PCHANNEL_DEF pChannel,
                                      INT channelCount,
                                      ULONG versionRequested,
                                      PCHANNEL_INIT_EVENT_EX_FN pChannelInitEventProcEx
                                      );

typedef VirtualChannelInitEx_Freerdp PVIRTUALCHANNELINITEX_FREERDP;

typedef UINT (*VirtualChannelInitEx_Windows) (LPVOID lpUserParam,
                                              LPVOID pInitHandle,
                                              PCHANNEL_DEF pChannel,
                                              INT channelCount,
                                              ULONG versionRequested,
                                              PCHANNEL_INIT_EVENT_EX_FN pChannelInitEventProcEx
                                      );

typedef VirtualChannelInitEx_Windows PVIRTUALCHANNELINITEX_WINDOWS;

typedef VOID (*VirtualChannelOpenEventEx) (LPVOID lpUserParam,
                                           DWORD openHandle,
                                           UINT event,
                                           LPVOID pData,
                                           UINT32 dataLength,
                                           UINT32 totalLength,
                                           UINT32 dataFlags
                                           );

typedef VirtualChannelOpenEventEx PCHANNEL_OPEN_EVENT_EX_FN;

typedef UINT (*VirtualChannelOpenEx) (LPVOID pInitHandle,
                                      LPDWORD pOpenHandle,
                                      PCHAR pChannelName,
                                      PCHANNEL_OPEN_EVENT_EX_FN pChannelOpenEventProcEx
                                      );

typedef VirtualChannelOpenEx PVIRTUALCHANNELOPENEX;

typedef UINT (*VirtualChannelCloseEx) (LPVOID pInitHandle,
                                       DWORD openHandle);

typedef VirtualChannelCloseEx PVIRTUALCHANNELCLOSEEX;

typedef UINT (*VirtualChannelWriteEx) (LPVOID pInitHandle,
                                       DWORD openHandle,
                                       LPVOID pData,
                                       ULONG dataLength,
                                       LPVOID pUserData
                                       );

typedef VirtualChannelWriteEx PVIRTUALCHANNELWRITEEX;

typedef struct  _CHANNEL_ENTRY_POINTS_EX_FREERDP {
        DWORD                  cbSize;
        DWORD                  protocolVersion;
        PVIRTUALCHANNELINITEX_FREERDP  pVirtualChannelInitEx;
        PVIRTUALCHANNELOPENEX  pVirtualChannelOpenEx;
        PVIRTUALCHANNELCLOSEEX pVirtualChannelCloseEx;
        PVIRTUALCHANNELWRITEEX pVirtualChannelWriteEx;
} CHANNEL_ENTRY_POINTS_EX_FREERDP, *PCHANNEL_ENTRY_POINTS_EX_FREERDP;

typedef BOOL (*VirtualChannelEntryEx_Freerdp) (PCHANNEL_ENTRY_POINTS_EX_FREERDP pEntryPointsEx,
                                               PVOID pInitHandle);

typedef VirtualChannelEntryEx_Freerdp PVIRTUALCHANNELENTRYEX_FREERDP;

typedef struct  _CHANNEL_ENTRY_POINTS_EX_WINDOWS {
        DWORD                  cbSize;
        DWORD                  protocolVersion;
        PVIRTUALCHANNELINITEX_WINDOWS  pVirtualChannelInitEx;
        PVIRTUALCHANNELOPENEX  pVirtualChannelOpenEx;
        PVIRTUALCHANNELCLOSEEX pVirtualChannelCloseEx;
        PVIRTUALCHANNELWRITEEX pVirtualChannelWriteEx;
} CHANNEL_ENTRY_POINTS_EX_WINDOWS, *PCHANNEL_ENTRY_POINTS_EX_WINDOWS;

typedef BOOL (*VirtualChannelEntryEx_Windows) (PCHANNEL_ENTRY_POINTS_EX_WINDOWS pEntryPointsEx,
                                               PVOID pInitHandle);

typedef VirtualChannelEntryEx_Windows PVIRTUALCHANNELENTRYEX_WINDOWS;

/*
 * MS compatible SVC plugin interface
 * Reference: http://msdn.microsoft.com/en-us/library/aa383580.aspx
 */

#define CHANNEL_RC_OK                           0

#define VIRTUAL_CHANNEL_VERSION_WIN2000         1

#define CHANNEL_MAX_COUNT 30


//#define CHANNEL_CHUNK_LENGTH 1600


/*
 * Static Virtual Channel Events
 */

enum RDP_SVC_CHANNEL_EVENT
{
   CHANNEL_EVENT_INITIALIZED = 0,
   CHANNEL_EVENT_CONNECTED = 1,
   CHANNEL_EVENT_V1_CONNECTED = 2,
   CHANNEL_EVENT_DISCONNECTED = 3,
   CHANNEL_EVENT_TERMINATED = 4,
   CHANNEL_EVENT_DATA_RECEIVED = 10,
   CHANNEL_EVENT_WRITE_COMPLETE = 11,
   CHANNEL_EVENT_WRITE_CANCELLED = 12,
   CHANNEL_EVENT_USER = 1000
};

/*
 * Static Virtual Channel Flags
 */

enum RDP_SVC_CHANNEL_FLAG
{
   CHANNEL_FLAG_MIDDLE = 0,
   CHANNEL_FLAG_FIRST = 0x01,
   CHANNEL_FLAG_LAST = 0x02,
   CHANNEL_FLAG_ONLY = (CHANNEL_FLAG_FIRST | CHANNEL_FLAG_LAST),
   CHANNEL_FLAG_SHOW_PROTOCOL = 0x10,
   CHANNEL_FLAG_SUSPEND = 0x20,
   CHANNEL_FLAG_RESUME = 0x40,
   CHANNEL_FLAG_FAIL = 0x100
};
